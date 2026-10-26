import { useCallback, useEffect, useState } from 'react'
import { useStore } from '@/main/store'
import {
  EVENT,
  formatCmdError,
  type ModelJobStatus,
  ModelPhase,
  type ModelStatus,
  type ModelsList,
  modelCancel,
  modelDownload,
  modelOpenDir,
  modelRemove,
  modelSelect,
  modelsList,
  on,
} from '@/shared/ipc'
import { ModelKind, TaskStatus } from '@/shared/types'
import { Alert } from '@/ui/Alert'
import { Button } from '@/ui/Button'
import { Icon } from '@/ui/Icon'
import { Page } from '@/ui/Page'
import { ProgressBar } from '@/ui/ProgressBar'
import { SettingsGroup, SettingsRow } from '@/ui/Settings'
import { StatusBadge } from '@/ui/StatusBadge'

interface Failure {
  modelId: string
  message: string
}

export function ModelsPage() {
  const { status, setError } = useStore()
  const capturing = status === TaskStatus.Capturing

  const [list, setList] = useState<ModelsList | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [job, setJob] = useState<ModelJobStatus | null>(null)
  const [failure, setFailure] = useState<Failure | null>(null)
  const [busyId, setBusyId] = useState<string | null>(null)
  const [confirmRemove, setConfirmRemove] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      const next = await modelsList()
      setList(next)
      setJob(next.job ?? null)
      setLoadError(null)
    } catch (e) {
      setLoadError(formatCmdError(e))
    }
  }, [])

  function retry() {
    setLoadError(null)
    void refresh()
  }

  useEffect(() => {
    let disposed = false
    void refresh()

    const unlisten = on<ModelJobStatus>(EVENT.modelsProgress, (payload) => {
      if (disposed) return
      const terminal =
        payload.phase === ModelPhase.Done ||
        payload.phase === ModelPhase.Failed ||
        payload.phase === ModelPhase.Cancelled

      if (!terminal) {
        setJob(payload)
        return
      }

      if (payload.phase === ModelPhase.Failed) {
        setFailure({
          modelId: payload.modelId,
          message: payload.error ?? 'Ошибка загрузки',
        })
      }
      setJob(null)
      void refresh()
    })

    return () => {
      disposed = true
      void unlisten.then((fn) => fn())
    }
  }, [refresh])

  async function download(modelId: string) {
    setFailure(null)
    setBusyId(modelId)
    try {
      await modelDownload(modelId)
      await refresh()
    } catch (e) {
      setError(formatCmdError(e))
    } finally {
      setBusyId(null)
    }
  }

  async function cancel(jobId: string) {
    try {
      await modelCancel(jobId)
    } catch (e) {
      setError(formatCmdError(e))
    }
  }

  async function select(modelId: string) {
    setBusyId(modelId)
    try {
      setList(await modelSelect(modelId))
      setConfirmRemove(null)
    } catch (e) {
      setError(formatCmdError(e))
    } finally {
      setBusyId(null)
    }
  }

  async function remove(modelId: string) {
    if (confirmRemove !== modelId) {
      setConfirmRemove(modelId)
      return
    }
    setBusyId(modelId)
    setConfirmRemove(null)
    try {
      setList(await modelRemove(modelId))
    } catch (e) {
      setError(formatCmdError(e))
    } finally {
      setBusyId(null)
    }
  }

  function openModelsDir() {
    void modelOpenDir().catch((e) => setError(formatCmdError(e)))
  }

  if (!list) {
    return (
      <Page title="Модели">
        {loadError ? (
          <Alert tone="error" title="Не удалось прочитать список моделей">
            <p className="font-mono">{loadError}</p>
            <Button size="sm" className="mt-2" onClick={retry}>
              Повторить
            </Button>
          </Alert>
        ) : (
          <p className="text-[var(--color-ink-2)] text-sm">Загрузка…</p>
        )}
      </Page>
    )
  }

  const connected = list.userRoot !== undefined
  const brokenSelected = [...list.asr, ...list.mt].find((m) => m.selected && !m.complete)

  return (
    <Page title="Модели">
      {loadError && (
        <Alert tone="error" title="Не удалось обновить список моделей">
          <p className="font-mono">{loadError}</p>
          <Button size="sm" className="mt-2" onClick={retry}>
            Повторить
          </Button>
        </Alert>
      )}

      {brokenSelected && (
        <Alert tone="error" title={`Нет файлов выбранной модели «${brokenSelected.title}»`}>
          {brokenSelected.bundled
            ? 'Файлы встроенной модели отсутствуют. Переустановите приложение.'
            : 'Загрузите модель заново или выберите другую установленную модель.'}
        </Alert>
      )}

      {capturing && (
        <Alert tone="info" title="Идёт распознавание">
          Новая модель будет использована при следующем запуске. Для удаления модели остановите запись.
        </Alert>
      )}

      {failure && (
        <Alert tone="error" title={`${modelTitle(list, failure.modelId)} Ошибка загрузки`}>
          {failure.message}
          <button type="button" className="ml-2 underline underline-offset-2" onClick={() => setFailure(null)}>
            Понятно
          </button>
        </Alert>
      )}

      {[
        { title: 'Распознавание речи', items: list.asr },
        { title: 'Машинный перевод', items: list.mt },
      ].map((group) => (
        <SettingsGroup key={group.title} title={group.title}>
          {group.items.map((m) => (
            <ModelRow
              key={m.id}
              model={m}
              job={job}
              capturing={capturing}
              busy={busyId === m.id}
              confirmRemove={confirmRemove === m.id}
              onDownload={() => void download(m.id)}
              onCancel={() => job && void cancel(job.jobId)}
              onSelect={() => void select(m.id)}
              onRemove={() => void remove(m.id)}
              onCancelRemove={() => setConfirmRemove(null)}
            />
          ))}
        </SettingsGroup>
      ))}

      <p className="px-1 text-[var(--color-ink-2)] text-[0.7rem] leading-relaxed">
        {connected
          ? 'Для загрузки дополнительных моделей нужен интернет.'
          : 'Папка моделей недоступна. Дополнительные модели пока нельзя использовать.'}
        {connected && (
          <button type="button" className="ml-1 underline underline-offset-2" onClick={openModelsDir}>
            Открыть папку моделей
          </button>
        )}
      </p>
    </Page>
  )
}

interface RowProps {
  model: ModelStatus
  job: ModelJobStatus | null
  capturing: boolean
  busy: boolean
  confirmRemove: boolean
  onDownload: () => void
  onCancel: () => void
  onSelect: () => void
  onRemove: () => void
  onCancelRemove: () => void
}

function ModelRow({
  model,
  job,
  capturing,
  busy,
  confirmRemove,
  onDownload,
  onCancel,
  onSelect,
  onRemove,
  onCancelRemove,
}: RowProps) {
  const activeJob = job && job.modelId === model.id ? job : null
  const licenses = [...new Set(model.licenses.map((l) => l.license))]
  const needsAttribution = model.licenses.some((l) => l.requiresAttribution)
  const canDelete = model.installed && !model.bundled && model.downloadable && !model.selected && !capturing && !job

  return (
    <SettingsRow
      icon={model.kind === ModelKind.Asr ? 'waveform' : 'translate'}
      title={
        <span className="flex flex-wrap items-center gap-1.5">
          {model.title}
          {model.selected && (
            <span
              role="img"
              aria-label="Выбрана"
              title="Выбрана"
              className="inline-flex items-center text-[var(--color-accent)]"
            >
              <Icon name="check" size={15} strokeWidth={2.25} />
            </span>
          )}
          {model.bundled && <StatusBadge tone="muted">Встроенная</StatusBadge>}
          {!model.selected && statusBadge(model)}
        </span>
      }
      description={
        <span>
          Около {formatBytes(model.installedBytes)}
          {licenses.length > 0 &&
            ` · ${licenses.join(' / ')}${needsAttribution ? ' (требуется указание авторства)' : ''}`}
          {!model.bundled &&
            model.installed &&
            !model.complete &&
            ' · Файлы повреждены или отсутствуют. Загрузите модель заново'}
          {model.bundled && !model.complete && ' · Встроенные файлы отсутствуют. Переустановите приложение'}
          {!model.bundled && !model.downloadable && ' · Модель ещё не опубликована'}
        </span>
      }
      control={
        <div className="flex items-center gap-2">
          {activeJob ? (
            <>
              <div className="flex flex-col items-end gap-1">
                <ProgressBar value={activeJob.percent ?? null} label={`${model.title} Ход загрузки`} />
                <span className="font-mono text-[0.65rem] text-[var(--color-ink-2)]">
                  {phaseLabel(activeJob.phase)}
                  {activeJob.percent != null && ` ${Math.round(activeJob.percent)}%`}
                </span>
              </div>
              <Button size="sm" variant="ghost" onClick={onCancel}>
                Отмена
              </Button>
            </>
          ) : (
            <>
              {model.selected ? null : model.installed && model.complete ? (
                <Button size="sm" onClick={onSelect} disabled={busy}>
                  Выбрать
                </Button>
              ) : model.downloadable ? (
                <Button
                  size="sm"
                  variant="primary"
                  icon="download"
                  onClick={onDownload}
                  disabled={busy || job !== null}
                >
                  {model.installed ? 'Загрузить заново' : 'Загрузить'}
                </Button>
              ) : null}

              {canDelete &&
                (confirmRemove ? (
                  <>
                    <Button size="sm" variant="danger" onClick={onRemove} disabled={busy}>
                      Подтвердить удаление
                    </Button>
                    <Button size="sm" variant="ghost" onClick={onCancelRemove}>
                      Отмена
                    </Button>
                  </>
                ) : (
                  <Button size="sm" variant="ghost" onClick={onRemove} disabled={busy}>
                    Удалить
                  </Button>
                ))}
            </>
          )}
        </div>
      }
    />
  )
}

function statusBadge(model: ModelStatus) {
  if (model.complete) return <StatusBadge tone="ok">{model.bundled ? 'Встроенная' : 'Загружена'}</StatusBadge>
  if (model.bundled) return <StatusBadge tone="danger">Нет встроенных файлов</StatusBadge>
  if (model.installed) return <StatusBadge tone="danger">Неполные файлы</StatusBadge>
  if (model.downloadable) return <StatusBadge tone="warn">Не установлена</StatusBadge>
  return <StatusBadge tone="muted">Ещё не опубликована</StatusBadge>
}

function phaseLabel(phase: ModelPhase): string {
  switch (phase) {
    case ModelPhase.Downloading:
      return 'Загрузка'
    case ModelPhase.Verifying:
      return 'Проверка'
    case ModelPhase.Extracting:
      return 'Распаковка'
    case ModelPhase.Validating:
      return 'Проверка файлов'
    case ModelPhase.Done:
      return 'Готово'
    case ModelPhase.Cancelled:
      return 'Отменено'
    case ModelPhase.Failed:
      return 'Ошибка'
  }
}

function modelTitle(list: ModelsList, id: string): string {
  return [...list.asr, ...list.mt].find((m) => m.id === id)?.title ?? id
}

function formatBytes(bytes: number): string {
  if (!bytes) return '—'
  const mb = bytes / (1024 * 1024)
  if (mb >= 1024) return `${(mb / 1024).toLocaleString('ru-RU', { maximumFractionDigits: 1 })} ГБ`
  if (mb >= 10) return `${Math.round(mb).toLocaleString('ru-RU')} МБ`
  return `${mb.toLocaleString('ru-RU', { minimumFractionDigits: 1, maximumFractionDigits: 1 })} МБ`
}
