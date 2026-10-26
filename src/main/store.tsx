import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react'
import {
  type AppInfo,
  AudioSourceKind,
  type AudioStatus,
  appInfo,
  audioDevices,
  type CmdPermissionError,
  EVENT,
  formatCmdError,
  type HotkeyFailure,
  type HotkeyInfo,
  hotkeyFailures,
  type InputDevice,
  type OverlayConfig,
  on,
  overlayConfigGet,
  overlayConfigSet,
  overlayIsVisible,
  overlayToggle,
  type PermissionSnapshot,
  type Preferences,
  type PrefsPatch,
  permissionOf,
  permissionStatus,
  prefsGet,
  prefsSet,
  sessionClear,
  sessionStart,
  sessionStatus,
  sessionStop,
  shortcutsGet,
  Theme,
  TranslateEngine,
  type UpdateCheckResult,
  type UpdateProgress,
  updateCheck,
  updateInstall,
  updateRestart,
} from '@/shared/ipc'
import { withTimeout } from '@/shared/timeout'
import { TaskStatus } from '@/shared/types'

type EndpointConfig = Preferences['endpoint']

export type UpdatePhase = 'idle' | 'checking' | 'downloading' | 'ready' | 'error'

interface Store {
  info: AppInfo | null

  prefsReady: boolean
  audioSource: AudioSourceKind
  device: string
  translateEngine: TranslateEngine
  sourceLang: string
  theme: Theme
  endpoint: EndpointConfig

  status: TaskStatus
  activeDevice: string | null
  audio: AudioStatus | null

  devices: InputDevice[]
  deviceError: string | null

  overlayCfg: OverlayConfig | null
  overlayVisible: boolean

  hotkeys: HotkeyInfo[]
  hotkeyProblems: HotkeyFailure[]

  updateInfo: UpdateCheckResult | null
  updatePhase: UpdatePhase
  updateProgress: UpdateProgress | null
  updateError: string | null
  installedVersion: string | null
  checkUpdate: (silent?: boolean) => Promise<void>
  installUpdate: () => Promise<void>
  restartApp: () => Promise<void>

  onboardingDone: boolean
  completeOnboarding: () => Promise<void>

  permissions: PermissionSnapshot | null
  refreshPermission: () => Promise<void>
  permissionError: CmdPermissionError | null
  setPermissionError: (e: CmdPermissionError | null) => void

  error: string | null
  setError: (e: string | null) => void
  pending: boolean

  toggleSession: () => Promise<void>
  clearAll: () => Promise<void>
  setAudioSource: (v: AudioSourceKind) => void
  setDevice: (v: string) => void
  setTranslateEngine: (v: TranslateEngine) => void
  setDirection: (lang: string) => void
  changeTheme: (v: Theme) => Promise<void>
  setEndpoint: (v: EndpointConfig) => Promise<void>
  updateOverlay: (patch: Partial<OverlayConfig>) => Promise<void>
  toggleOverlay: () => Promise<void>
  refreshDevices: () => Promise<void>
  refreshHotkeys: () => Promise<void>
}

const STOP_TIMEOUT_MS = 8_000

const START_TIMEOUT_MS = 60_000

const Ctx = createContext<Store | null>(null)

export function useStore(): Store {
  const s = useContext(Ctx)
  if (!s) throw new Error('useStore должен вызываться внутри <StoreProvider>')
  return s
}

export function StoreProvider({ children }: { children: ReactNode }) {
  const [info, setInfo] = useState<AppInfo | null>(null)
  const [prefsReady, setPrefsReady] = useState(false)

  const [audioSource, setAudioSource] = useState<AudioSourceKind>(AudioSourceKind.Loopback)
  const [device, setDevice] = useState('')
  const [translateEngine, setTranslateEngine] = useState<TranslateEngine>(TranslateEngine.LocalNmt)
  const [sourceLang, setSourceLang] = useState('en')
  const [theme, setTheme] = useState<Theme>(Theme.System)
  const [endpoint, setEndpoint] = useState<EndpointConfig>({
    trailingSilenceMs: 800,
    maxUtteranceMs: 8000,
  })

  const [status, setStatus] = useState<TaskStatus>(TaskStatus.Ready)
  const [activeDevice, setActiveDevice] = useState<string | null>(null)
  const [audio, setAudio] = useState<AudioStatus | null>(null)

  const [devices, setDevices] = useState<InputDevice[]>([])
  const [deviceError, setDeviceError] = useState<string | null>(null)

  const [overlayCfg, setOverlayCfg] = useState<OverlayConfig | null>(null)
  const [overlayVisible, setOverlayVisible] = useState(false)

  const [hotkeys, setHotkeys] = useState<HotkeyInfo[]>([])
  const [hotkeyProblems, setHotkeyProblems] = useState<HotkeyFailure[]>([])

  const [onboardingDone, setOnboardingDone] = useState(false)
  const [permissions, setPermissions] = useState<PermissionSnapshot | null>(null)
  const [permissionError, setPermissionError] = useState<CmdPermissionError | null>(null)

  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)
  const pendingRef = useRef(false)

  const [updateInfo, setUpdateInfo] = useState<UpdateCheckResult | null>(null)
  const [updatePhase, setUpdatePhase] = useState<UpdatePhase>('idle')
  const [updateProgress, setUpdateProgress] = useState<UpdateProgress | null>(null)
  const [updateError, setUpdateError] = useState<string | null>(null)
  const [installedVersion, setInstalledVersion] = useState<string | null>(null)
  const updateBusyRef = useRef(false)

  useEffect(() => {
    appInfo()
      .then(setInfo)
      .catch((e) => setError(formatCmdError(e)))
    hotkeyFailures()
      .then(setHotkeyProblems)
      .catch(() => {})
    shortcutsGet()
      .then(setHotkeys)
      .catch(() => {})

    const unlisten: Array<() => void> = []
    let disposed = false
    const keep = (fn: () => void) => {
      if (disposed) fn()
      else unlisten.push(fn)
    }

    on<TaskStatus>(EVENT.taskState, (s) => {
      setStatus(s)
      if (s !== TaskStatus.Capturing) setAudio(null)
    }).then(keep)

    on<AudioStatus>(EVENT.audio, setAudio).then(keep)

    on<OverlayConfig>(EVENT.overlayState, setOverlayCfg).then(keep)

    on<boolean>(EVENT.overlayVisibility, setOverlayVisible).then(keep)

    on<UpdateProgress>(EVENT.updateProgress, setUpdateProgress).then(keep)

    return () => {
      disposed = true
      for (const fn of unlisten) fn()
    }
  }, [])

  useEffect(() => {
    prefsGet()
      .then((p) => {
        setAudioSource(p.audioSource as AudioSourceKind)
        setTranslateEngine(p.translateEngine)
        setSourceLang(p.sourceLang)
        setTheme(p.theme)
        setEndpoint(p.endpoint)
        setOnboardingDone(p.onboardingDone)
        if (p.deviceId) setDevice(p.deviceId)
      })
      .catch(() => {})
      .finally(() => setPrefsReady(true))

    overlayConfigGet()
      .then(setOverlayCfg)
      .catch(() => {})
    overlayIsVisible()
      .then(setOverlayVisible)
      .catch(() => {})
  }, [])

  const refreshPermission = useCallback(async () => {
    try {
      setPermissions(await permissionStatus())
    } catch {
      setPermissions(null)
    }
  }, [])

  useEffect(() => {
    void refreshPermission()
    const onFocus = () => void refreshPermission()
    window.addEventListener('focus', onFocus)
    return () => window.removeEventListener('focus', onFocus)
  }, [refreshPermission])

  const applyTheme = useCallback((pref: Theme) => {
    const systemDark = window.matchMedia('(prefers-color-scheme: dark)').matches
    const dark = pref === Theme.Dark || (pref === Theme.System && systemDark)
    document.documentElement.classList.toggle('theme-light', !dark)
    try {
      localStorage.setItem('audioscript.theme', pref)
    } catch {}
  }, [])

  useEffect(() => {
    applyTheme(theme)
    if (theme !== Theme.System) return
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const onChange = () => applyTheme(Theme.System)
    mq.addEventListener('change', onChange)
    return () => mq.removeEventListener('change', onChange)
  }, [theme, applyTheme])

  const patchPrefs = useCallback(async (patch: PrefsPatch) => {
    try {
      const p = await prefsSet(patch)
      if (patch.theme !== undefined) setTheme(p.theme)
      if (patch.endpoint !== undefined) setEndpoint(p.endpoint)
    } catch (e) {
      setError(formatCmdError(e))
    }
  }, [])

  const completeOnboarding = useCallback(async () => {
    setOnboardingDone(true)
    try {
      await prefsSet({ onboardingDone: true })
    } catch (e) {
      setError(formatCmdError(e))
    }
  }, [])

  const refreshDevices = useCallback(async () => {
    try {
      const list = await audioDevices(audioSource)
      setDevices(list)
      setDeviceError(null)
      setDevice((cur) => cur || (list.find((d) => d.isDefault)?.id ?? ''))
    } catch (e) {
      setDeviceError(formatCmdError(e))
    }
  }, [audioSource])

  useEffect(() => {
    if (!prefsReady) return
    refreshDevices()
  }, [prefsReady, refreshDevices])

  const toggleSession = useCallback(async () => {
    if (pendingRef.current) return
    pendingRef.current = true
    setPending(true)
    setError(null)
    setPermissionError(null)
    try {
      if (status === TaskStatus.Capturing) {
        setStatus(
          await withTimeout(
            sessionStop(),
            STOP_TIMEOUT_MS,
            'Не удалось дождаться остановки. Повторите попытку или выберите «Остановить запись» в области уведомлений',
          ),
        )
        setActiveDevice(null)
      } else {
        setStatus(
          await withTimeout(
            sessionStart(device || undefined, translateEngine, sourceLang, audioSource),
            START_TIMEOUT_MS,
            'Не удалось дождаться запуска записи. Повторите попытку',
          ),
        )
        setActiveDevice((await sessionStatus()).device ?? null)
        void refreshPermission()
      }
    } catch (e) {
      const denied = permissionOf(e)
      if (denied) {
        setPermissionError(denied)
        void refreshPermission()
      } else {
        setError(formatCmdError(e))
      }
      sessionStatus()
        .then((i) => setStatus(i.status))
        .catch(() => {})
    } finally {
      pendingRef.current = false
      setPending(false)
    }
  }, [status, device, translateEngine, sourceLang, audioSource, refreshPermission])

  const clearAll = useCallback(async () => {
    await sessionClear()
  }, [])

  const updateOverlay = useCallback(
    async (patch: Partial<OverlayConfig>) => {
      if (!overlayCfg) return
      try {
        setOverlayCfg(await overlayConfigSet({ ...overlayCfg, ...patch }))
      } catch (e) {
        setError(formatCmdError(e))
      }
    },
    [overlayCfg],
  )

  const toggleOverlay = useCallback(async () => {
    try {
      setOverlayVisible(await overlayToggle())
    } catch (e) {
      setError(formatCmdError(e))
    }
  }, [])

  const refreshHotkeys = useCallback(async () => {
    try {
      setHotkeys(await shortcutsGet())
      setHotkeyProblems(await hotkeyFailures())
    } catch (e) {
      setError(formatCmdError(e))
    }
  }, [])

  const changeTheme = useCallback(
    async (next: Theme) => {
      setTheme(next)
      await patchPrefs({ theme: next })
    },
    [patchPrefs],
  )

  const changeAudioSource = useCallback(
    (v: AudioSourceKind) => {
      setAudioSource(v)
      setDevice('')
      void patchPrefs({ audioSource: v, deviceId: '' })
    },
    [patchPrefs],
  )

  const changeDevice = useCallback(
    (v: string) => {
      setDevice(v)
      void patchPrefs({ deviceId: v })
    },
    [patchPrefs],
  )

  const changeTranslateEngine = useCallback(
    (v: TranslateEngine) => {
      setTranslateEngine(v)
      void patchPrefs({ translateEngine: v })
    },
    [patchPrefs],
  )

  const changeDirection = useCallback(
    (lang: string) => {
      setSourceLang(lang)
      void patchPrefs({ sourceLang: lang })
    },
    [patchPrefs],
  )

  const changeEndpoint = useCallback(
    async (v: EndpointConfig) => {
      setEndpoint(v)
      await patchPrefs({ endpoint: v })
    },
    [patchPrefs],
  )

  const checkUpdate = useCallback(async (silent = false) => {
    if (updateBusyRef.current) return
    updateBusyRef.current = true
    setUpdatePhase('checking')
    if (!silent) setUpdateError(null)
    try {
      setUpdateInfo(await updateCheck())
      setUpdatePhase('idle')
    } catch (e) {
      setUpdatePhase(silent ? 'idle' : 'error')
      if (!silent) setUpdateError(formatCmdError(e))
    } finally {
      updateBusyRef.current = false
    }
  }, [])

  const installUpdate = useCallback(async () => {
    if (updateBusyRef.current) return
    updateBusyRef.current = true
    setUpdatePhase('downloading')
    setUpdateError(null)
    setUpdateProgress({ downloaded: 0, total: null })
    try {
      const r = await updateInstall()
      setInstalledVersion(r.version)
      setUpdatePhase('ready')
    } catch (e) {
      setUpdatePhase('error')
      setUpdateError(formatCmdError(e))
    } finally {
      updateBusyRef.current = false
    }
  }, [])

  const restartApp = useCallback(async () => {
    try {
      await updateRestart()
    } catch (e) {
      setError(formatCmdError(e))
    }
  }, [])

  const autoCheckedRef = useRef(false)
  useEffect(() => {
    if (!info || autoCheckedRef.current) return
    autoCheckedRef.current = true
    if (!import.meta.env.PROD) return
    void checkUpdate(true)
  }, [info, checkUpdate])

  const value = useMemo<Store>(
    () => ({
      info,
      updateInfo,
      updatePhase,
      updateProgress,
      updateError,
      installedVersion,
      checkUpdate,
      installUpdate,
      restartApp,
      prefsReady,
      audioSource,
      device,
      translateEngine,
      sourceLang,
      theme,
      endpoint,
      status,
      activeDevice,
      audio,
      devices,
      deviceError,
      overlayCfg,
      overlayVisible,
      hotkeys,
      hotkeyProblems,
      onboardingDone,
      completeOnboarding,
      permissions,
      refreshPermission,
      permissionError,
      setPermissionError,
      error,
      setError,
      pending,
      toggleSession,
      clearAll,
      setAudioSource: changeAudioSource,
      setDevice: changeDevice,
      setTranslateEngine: changeTranslateEngine,
      setDirection: changeDirection,
      changeTheme,
      setEndpoint: changeEndpoint,
      updateOverlay,
      toggleOverlay,
      refreshDevices,
      refreshHotkeys,
    }),
    [
      info,
      updateInfo,
      updatePhase,
      updateProgress,
      updateError,
      installedVersion,
      checkUpdate,
      installUpdate,
      restartApp,
      prefsReady,
      audioSource,
      device,
      translateEngine,
      sourceLang,
      theme,
      endpoint,
      status,
      activeDevice,
      audio,
      devices,
      deviceError,
      overlayCfg,
      overlayVisible,
      hotkeys,
      hotkeyProblems,
      onboardingDone,
      completeOnboarding,
      permissions,
      refreshPermission,
      permissionError,
      error,
      pending,
      toggleSession,
      clearAll,
      changeAudioSource,
      changeDevice,
      changeTranslateEngine,
      changeDirection,
      changeTheme,
      changeEndpoint,
      updateOverlay,
      toggleOverlay,
      refreshDevices,
      refreshHotkeys,
    ],
  )

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>
}
