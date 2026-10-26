import { useEffect, useState } from 'react'
import { useStore } from '@/main/store'
import { formatCmdError, type ModelsList, modelsList, openRepo, openSponsor, openUrl } from '@/shared/ipc'
import { ModelKind, TaskStatus } from '@/shared/types'
import { Button } from '@/ui/Button'
import { Icon } from '@/ui/Icon'
import { Page } from '@/ui/Page'
import { ProgressBar } from '@/ui/ProgressBar'
import { SettingsGroup, SettingsRow } from '@/ui/Settings'

const SITE_URL = 'https://github.com/envOnion/AudioScript'
const UPSTREAM_URL = 'https://github.com/niyongsheng/AudioScript'

export function AboutPage() {
  const {
    info,
    setError,
    status,
    updateInfo,
    updatePhase,
    updateProgress,
    updateError,
    installedVersion,
    checkUpdate,
    installUpdate,
    restartApp,
  } = useStore()
  const [models, setModels] = useState<ModelsList | null>(null)
  const [modelsFailed, setModelsFailed] = useState(false)
  const [licensesOpen, setLicensesOpen] = useState(false)

  useEffect(() => {
    let disposed = false
    modelsList()
      .then((m) => {
        if (!disposed) setModels(m)
      })
      .catch(() => {
        if (!disposed) setModelsFailed(true)
      })
    return () => {
      disposed = true
    }
  }, [])

  async function openSite() {
    try {
      await openUrl(SITE_URL)
    } catch (e) {
      setError(formatCmdError(e))
    }
  }

  async function openSponsorPage() {
    try {
      await openSponsor()
    } catch (e) {
      setError(formatCmdError(e))
    }
  }

  const capturing = status === TaskStatus.Capturing
  const percent =
    updateProgress?.total != null
      ? Math.min(100, Math.floor((updateProgress.downloaded / updateProgress.total) * 100))
      : null

  const updateDescription = (() => {
    if (updatePhase === 'downloading') {
      return `Загрузка и установка v${updateInfo?.version ?? ''}${percent == null ? '' : ` (${percent}%)`}…`
    }
    if (updatePhase === 'ready') {
      const version = installedVersion ?? updateInfo?.version ?? ''
      return capturing
        ? `Установлена v${version}, остановите запись перед перезапуском`
        : `Установлена v${version}, перезапустите приложение для применения`
    }
    if (updatePhase === 'error') return `Ошибка обновления: ${updateError ?? 'Неизвестная ошибка'}`
    if (updatePhase === 'checking') return 'Проверка обновлений…'
    if (updateInfo?.available) {
      return capturing
        ? 'Доступна новая версия. Остановите запись перед обновлением'
        : `Доступна новая версия v${updateInfo.version}`
    }
    if (updateInfo) return `Установлена последняя версия (v${updateInfo.currentVersion})`
    return 'Проверяется при запуске. Можно проверить вручную'
  })()

  return (
    <Page title="О приложении">
      <SettingsGroup title="Приложение">
        <SettingsRow icon="info" title="Версия" control={<Mono>{info?.version ?? '—'}</Mono>} />
        <SettingsRow
          icon="download"
          title="Проверить обновления"
          description={
            <>
              {updateDescription}
              {updateInfo?.available && updatePhase === 'idle' && updateInfo.notes && (
                <p className="mt-1 line-clamp-3 whitespace-pre-line">{updateInfo.notes}</p>
              )}
            </>
          }
          control={
            updatePhase === 'downloading' ? (
              <div className="flex items-center gap-2">
                <ProgressBar value={percent} label="Загрузка обновления" />
                <Mono>{percent == null ? 'Загрузка' : `${percent}%`}</Mono>
              </div>
            ) : updatePhase === 'ready' ? (
              <Button size="sm" variant="primary" icon="refresh" onClick={() => void restartApp()} disabled={capturing}>
                Перезапустить
              </Button>
            ) : updateInfo?.available ? (
              <Button
                size="sm"
                variant="primary"
                icon="download"
                onClick={() => void installUpdate()}
                disabled={capturing || updatePhase === 'checking'}
              >
                Загрузить и установить
              </Button>
            ) : (
              <Button
                size="sm"
                icon="refresh"
                onClick={() => void checkUpdate()}
                disabled={!info || updatePhase === 'checking'}
                pending={updatePhase === 'checking'}
              >
                Проверить
              </Button>
            )
          }
        />
        <SettingsRow
          icon="chip"
          title="Платформа"
          control={
            <Mono>
              {info?.platform ?? '—'} · {info?.arch ?? '—'}
            </Mono>
          }
        />
        <SettingsRow
          icon="external"
          title="Проект на GitHub"
          onClick={() => void openSite()}
          control={<Icon name="chevron" size={16} className="mt-0.5 text-[var(--color-ink-2)]" />}
        />
      </SettingsGroup>

      <SettingsGroup title="Поддержать разработку">
        <SettingsRow
          icon="heart"
          title="Поддержать автора оригинала"
          description="Поддержать автора исходного проекта AudioScript на Afdian."
          onClick={() => void openSponsorPage()}
          control={<Icon name="external" size={16} className="text-[var(--color-ink-2)]" />}
        />
      </SettingsGroup>

      <SettingsGroup
        title="Открытый исходный код"
        footer="Русская версия: envOnion. Исходный проект AudioScript: niyongsheng."
      >
        <SettingsRow
          icon="github"
          title={
            <span className="flex items-center gap-2">
              Лицензия
              <Mono>MIT</Mono>
            </span>
          }
          description="Исходный код русской версии: envOnion/AudioScript"
          onClick={() => void openRepo().catch((e) => setError(formatCmdError(e)))}
          control={<Icon name="chevron" size={16} className="mt-0.5 text-[var(--color-ink-2)]" />}
        />
        <SettingsRow
          icon="github"
          title="Исходный проект"
          description="niyongsheng/AudioScript · MIT"
          onClick={() => void openUrl(UPSTREAM_URL).catch((e) => setError(formatCmdError(e)))}
          control={<Icon name="chevron" size={16} className="mt-0.5 text-[var(--color-ink-2)]" />}
        />
      </SettingsGroup>

      <SettingsGroup
        title="Модели и лицензии"
        footer={
          licensesOpen
            ? 'Веса OPUS-MT преобразованы в CTranslate2 int8, словари SentencePiece — в tokenizer JSON. Обучающие данные не изменялись.'
            : undefined
        }
      >
        <SettingsRow
          icon="shield"
          title="Лицензии моделей"
          onClick={() => setLicensesOpen((open) => !open)}
          control={
            <Icon
              name="chevron"
              size={16}
              className={`mt-0.5 text-[var(--color-ink-2)] transition-transform ${licensesOpen ? 'rotate-90' : ''}`}
            />
          }
        />
        {licensesOpen &&
          (models ? (
            [...models.asr, ...models.mt].flatMap((m) =>
              m.licenses.map((l) => (
                <SettingsRow
                  key={`${m.id}-${l.component}`}
                  icon={m.kind === ModelKind.Asr ? 'waveform' : 'translate'}
                  title={l.component}
                  description={
                    <>
                      {m.title} · {l.license}
                      {l.requiresAttribution && ' · Требуется указание авторства'}
                      {l.attribution && ` · ${l.attribution}`}
                    </>
                  }
                  control={
                    l.licenseUrl ? (
                      <Button
                        size="sm"
                        variant="ghost"
                        icon="external"
                        onClick={() => void openUrl(l.licenseUrl as string).catch((e) => setError(formatCmdError(e)))}
                      >
                        Лицензия
                      </Button>
                    ) : undefined
                  }
                />
              )),
            )
          ) : (
            <SettingsRow
              title={modelsFailed ? 'Не удалось загрузить лицензии. Повторите попытку в разделе «Модели»' : 'Загрузка…'}
            />
          ))}
      </SettingsGroup>
    </Page>
  )
}

function Mono({ children }: { children: React.ReactNode }) {
  return <span className="font-mono text-[var(--color-ink-1)] text-xs">{children}</span>
}
