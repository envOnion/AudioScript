import { useEffect, useState } from 'react'
import { useStore } from '@/main/store'
import { type ConfigDefaults, configDefaults, TranslateEngine } from '@/shared/ipc'
import {
  MAX_MAX_UTTERANCE_MS,
  MAX_TRAILING_SILENCE_MS,
  MIN_MAX_UTTERANCE_MS,
  MIN_TRAILING_SILENCE_MS,
  TaskStatus,
} from '@/shared/types'
import { Alert } from '@/ui/Alert'
import { Page } from '@/ui/Page'
import { Select } from '@/ui/Select'
import { SettingsGroup, SettingsRow } from '@/ui/Settings'
import { Slider } from '@/ui/Slider'

const ENGINE_OPTIONS = [
  { value: TranslateEngine.LocalNmt, label: 'Локальный (OPUS-MT)' },
  { value: TranslateEngine.Off, label: 'Выключен (только оригинал)' },
  {
    value: TranslateEngine.RemoteApi,
    label: 'Онлайн API',
    unavailableReason: 'Пока недоступен',
  },
]

const MOCK_OPTION = { value: TranslateEngine.Mock, label: 'Тестовый движок' }

const DIRECTION_OPTIONS = [
  { value: 'en', label: 'Английский → русский' },
  { value: 'ru', label: 'Русский → английский' },
]

export function TranslatePage() {
  const { translateEngine, setTranslateEngine, sourceLang, setDirection, endpoint, setEndpoint, status } = useStore()
  const capturing = status === TaskStatus.Capturing

  const [defaults, setDefaults] = useState<ConfigDefaults | null>(null)
  useEffect(() => {
    configDefaults()
      .then(setDefaults)
      .catch(() => {})
  }, [])

  const [draft, setDraft] = useState(endpoint)
  useEffect(() => setDraft(endpoint), [endpoint])

  return (
    <Page title="Перевод">
      {capturing && <Alert tone="info">Параметры разбиения применятся при следующем запуске записи.</Alert>}

      <SettingsGroup title="Движок">
        <SettingsRow
          icon="translate"
          title="Движок перевода"
          description="Работает без интернета. Звук и текст остаются на устройстве."
          control={
            <Select
              label="Движок перевода"
              value={translateEngine}
              options={translateEngine === TranslateEngine.Mock ? [...ENGINE_OPTIONS, MOCK_OPTION] : ENGINE_OPTIONS}
              onChange={setTranslateEngine}
              disabled={capturing}
            />
          }
        />
        <SettingsRow
          icon="external"
          title="Направление перевода"
          description="Выберите язык речи. Он используется и для распознавания без перевода."
          control={
            <Select
              label="Направление перевода"
              value={sourceLang}
              options={DIRECTION_OPTIONS}
              onChange={setDirection}
              disabled={capturing}
            />
          }
        />
      </SettingsGroup>

      <SettingsGroup title="Разбиение на фразы" footer="Чем длиннее фраза, тем позже появляется перевод.">
        <SettingsRow
          icon="waveform"
          title="Пауза в конце фразы"
          description="Меньшее значение ускоряет перевод, но делит речь на более короткие фразы."
          control={
            <div className="w-64">
              <Slider
                label="Пауза в конце"
                value={draft.trailingSilenceMs}
                min={MIN_TRAILING_SILENCE_MS}
                max={MAX_TRAILING_SILENCE_MS}
                step={50}
                format={(v) => `${v.toLocaleString('ru-RU')} мс`}
                onChange={(v) => setDraft((d) => ({ ...d, trailingSilenceMs: v }))}
                onCommit={(v) => void setEndpoint({ ...draft, trailingSilenceMs: v })}
                defaultValue={defaults?.endpoint.trailingSilenceMs}
                disabled={capturing}
              />
            </div>
          }
        />
        <SettingsRow
          icon="alert"
          title="Максимальная длина фразы"
          description="После этого времени фраза завершается принудительно. Значение должно быть больше паузы в конце."
          control={
            <div className="w-64">
              <Slider
                label="Максимальная длина фразы"
                value={draft.maxUtteranceMs}
                min={MIN_MAX_UTTERANCE_MS}
                max={MAX_MAX_UTTERANCE_MS}
                step={500}
                format={(v) => `${(v / 1000).toLocaleString('ru-RU', { minimumFractionDigits: 1 })} с`}
                onChange={(v) => setDraft((d) => ({ ...d, maxUtteranceMs: v }))}
                onCommit={(v) => void setEndpoint({ ...draft, maxUtteranceMs: v })}
                defaultValue={defaults?.endpoint.maxUtteranceMs}
                disabled={capturing}
              />
            </div>
          }
        />
        <SettingsRow
          icon="chip"
          title="Режим"
          control={
            <div className="flex gap-1.5">
              <button
                type="button"
                disabled={capturing || !defaults}
                onClick={() => defaults && void setEndpoint(defaults.endpointProfiles.interpretation)}
                className="rounded-[var(--radius-ctl)] border border-[var(--color-border-subtle)] px-2 py-1 text-[var(--color-ink-1)] text-xs transition hover:bg-[var(--color-surface-2)] disabled:opacity-40"
              >
                Синхронный перевод
              </button>
              <button
                type="button"
                disabled={capturing || !defaults}
                onClick={() => defaults && void setEndpoint(defaults.endpointProfiles.dictation)}
                className="rounded-[var(--radius-ctl)] border border-[var(--color-border-subtle)] px-2 py-1 text-[var(--color-ink-1)] text-xs transition hover:bg-[var(--color-surface-2)] disabled:opacity-40"
              >
                Диктовка
              </button>
            </div>
          }
        />
      </SettingsGroup>
    </Page>
  )
}
