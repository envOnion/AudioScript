import { useCallback, useState } from 'react'
import { useStore } from '@/main/store'
import {
  formatCmdError,
  MicPermission,
  openPrivacySettings,
  type PrivacyPane,
  permissionProbeSystemAudio,
  permissionRequestMic,
  type SystemAudioProbe,
  SystemAudioVerdict,
} from '@/shared/ipc'
import { isWindows } from '@/shared/platform'
import { TaskStatus } from '@/shared/types'
import { Alert } from '@/ui/Alert'
import { Button } from '@/ui/Button'
import { LevelBars, LevelMeter } from '@/ui/LevelMeter'
import { SettingsGroup, SettingsRow } from '@/ui/Settings'
import { StatusBadge } from '@/ui/StatusBadge'

const MIC_TEXT: Record<MicPermission, string> = {
  granted: 'Разрешено',
  denied: 'Запрещено',
  restricted: 'Ограничено',
  notDetermined: 'Нет разрешения',
  notApplicable: 'Не требуется',
  unknown: 'Неизвестно',
}

const MIC_TONE: Record<MicPermission, 'ok' | 'warn' | 'danger' | 'muted'> = {
  granted: 'ok',
  denied: 'danger',
  restricted: 'danger',
  notDetermined: 'warn',
  notApplicable: 'muted',
  unknown: 'muted',
}

const PROBE_TEXT: Record<SystemAudioVerdict, string> = {
  audible: 'Доступно',
  silentWhilePlaying: 'Нет разрешения',
  tapRejected: 'Запрещено',
  noAudioPlaying: 'Не проверено',
  unsupportedOs: 'Устаревшая ОС',
  inconclusive: 'Неизвестно',
}

const PROBE_TONE: Record<SystemAudioVerdict, 'ok' | 'warn' | 'danger' | 'muted'> = {
  audible: 'ok',
  silentWhilePlaying: 'danger',
  tapRejected: 'danger',
  noAudioPlaying: 'muted',
  unsupportedOs: 'warn',
  inconclusive: 'muted',
}

export function PermissionPanel() {
  const { permissions, refreshPermission, refreshDevices, status } = useStore()
  const capturing = status === TaskStatus.Capturing
  const [probing, setProbing] = useState(false)
  const [requesting, setRequesting] = useState(false)
  const [probe, setProbe] = useState<SystemAudioProbe | null>(null)
  const [note, setNote] = useState<string | null>(null)
  const [failure, setFailure] = useState<string | null>(null)

  const mic = permissions?.microphone ?? MicPermission.Unknown
  const promptable = permissions?.microphonePromptable ?? false

  const runProbe = useCallback(async () => {
    setProbing(true)
    setNote(null)
    setFailure(null)
    try {
      const result = await permissionProbeSystemAudio()
      setProbe(result)
    } catch (e) {
      setFailure(formatCmdError(e))
    } finally {
      setProbing(false)
    }
  }, [])

  const askMic = useCallback(async () => {
    setRequesting(true)
    setNote(null)
    setFailure(null)
    try {
      const result = await permissionRequestMic()
      if (result.timedOut) {
        setNote('Время ожидания системного запроса истекло. Если он ещё открыт, ответьте на него и повторите проверку.')
      }
      await refreshPermission()
      if (result.state === MicPermission.Granted) await refreshDevices()
    } catch (e) {
      setFailure(formatCmdError(e))
    } finally {
      setRequesting(false)
    }
  }, [refreshPermission, refreshDevices])

  const open = useCallback(async (pane: PrivacyPane) => {
    try {
      await openPrivacySettings(pane)
    } catch (e) {
      setFailure(formatCmdError(e))
    }
  }, [])

  return (
    <div className="space-y-3">
      {failure && (
        <Alert tone="error" title="Не удалось выполнить действие">
          <p className="font-mono text-[0.95em]">{failure}</p>
        </Alert>
      )}
      {note && <Alert tone="info">{note}</Alert>}

      <SettingsGroup
        title="Разрешения"
        footer="Звук обрабатывается на устройстве и не отправляется в сеть. После переустановки разрешения могут потребоваться снова."
      >
        <SettingsRow
          icon="waveform"
          title="Микрофон"
          description={micDescription(mic, promptable)}
          control={
            <div className="flex items-center gap-1.5">
              <StatusBadge tone={MIC_TONE[mic]}>{MIC_TEXT[mic]}</StatusBadge>
              {mic === MicPermission.NotDetermined && promptable && (
                <Button size="sm" pending={requesting} onClick={() => void askMic()}>
                  Запросить доступ
                </Button>
              )}
              {(mic === MicPermission.Denied ||
                mic === MicPermission.Restricted ||
                (mic === MicPermission.NotDetermined && !promptable)) && (
                <Button size="sm" onClick={() => void open('microphone')}>
                  Открыть настройки
                </Button>
              )}
            </div>
          }
        />

        <SettingsRow
          icon="speaker"
          title="Запись системного звука"
          description={
            isWindows ? (
              'В Windows проверка системного звука пока недоступна. Запись системного звука работает.'
            ) : (
              <>
                {probing ? 'Проверка системного звука. Не останавливайте воспроизведение.' : probeDescription(probe)}
                {probing && (
                  <span className="mt-1 flex items-center gap-2">
                    <LevelMeter active />
                    <span className="font-mono">Уровень сигнала</span>
                  </span>
                )}
                {!probing && probe && (
                  <span className="mt-1 flex items-center gap-2">
                    <LevelBars level={probe.peak} />
                    <span className="font-mono">
                      Пик {formatDbfs(probe.peak)} · Среднее {formatDbfs(probe.rms)}
                    </span>
                  </span>
                )}
              </>
            )
          }
          control={
            isWindows ? (
              <StatusBadge tone="muted">Недоступно</StatusBadge>
            ) : (
              <div className="flex items-center gap-1.5">
                <StatusBadge tone={probe ? PROBE_TONE[probe.verdict] : 'muted'}>
                  {probe ? PROBE_TEXT[probe.verdict] : 'Не проверено'}
                </StatusBadge>
                {(probe?.verdict === SystemAudioVerdict.SilentWhilePlaying ||
                  probe?.verdict === SystemAudioVerdict.TapRejected) && (
                  <Button size="sm" onClick={() => void open('systemAudio')}>
                    Открыть настройки
                  </Button>
                )}
                <Button size="sm" pending={probing} disabled={capturing || probing} onClick={() => void runProbe()}>
                  Проверить
                </Button>
              </div>
            )
          }
        />
      </SettingsGroup>
    </div>
  )
}

function micDescription(mic: MicPermission, promptable: boolean): string {
  if (mic === MicPermission.Granted) return 'Нужен для записи с микрофона.'
  if (mic === MicPermission.Denied || mic === MicPermission.Restricted) {
    return 'Включите доступ вручную в настройках системы: повторного запроса не будет.'
  }
  if (mic === MicPermission.NotDetermined && !promptable) {
    return 'Запустите установленное приложение AudioScript, чтобы запросить доступ к микрофону.'
  }
  if (mic === MicPermission.NotDetermined) return 'Без доступа к микрофону macOS может скрывать список устройств.'
  return 'Не удалось проверить доступ к микрофону.'
}

function formatDbfs(value: number): string {
  if (value <= 0) return 'Тишина'
  const db = 20 * Math.log10(value)
  if (db <= -60) return '< -60 dBFS'
  return `${Math.round(db)} dBFS`
}

function probeDescription(probe: SystemAudioProbe | null): string {
  if (!probe) return 'Включите воспроизведение звука и нажмите «Проверить».'
  switch (probe.verdict) {
    case SystemAudioVerdict.Audible:
      return 'Звук поступает. Доступ разрешён.'
    case SystemAudioVerdict.SilentWhilePlaying:
      return 'Воспроизведение идёт, но приложение получает тишину. Обычно это означает, что доступ запрещён.'
    case SystemAudioVerdict.TapRejected:
      return 'Система запретила запись системного звука.'
    case SystemAudioVerdict.NoAudioPlaying:
      return 'Звук не воспроизводится. Включите его и повторите проверку.'
    case SystemAudioVerdict.UnsupportedOs:
      return 'Требуется macOS 14.4 или новее.'
    case SystemAudioVerdict.Inconclusive:
      return probe.detail ?? 'Не удалось определить результат. Повторите попытку.'
  }
}
