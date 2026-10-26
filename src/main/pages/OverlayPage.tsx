import { useEffect, useRef, useState } from 'react'
import { useStore } from '@/main/store'
import { formatCmdError, type OverlayConfig, overlaySetMode, shortcutsSet } from '@/shared/ipc'
import { isMacOS } from '@/shared/platform'
import { MAX_FONT_SIZE, MAX_OVERLAY_LINES, MIN_FONT_SIZE, OverlayMode } from '@/shared/types'
import { Button } from '@/ui/Button'
import { Page } from '@/ui/Page'
import { Select } from '@/ui/Select'
import { SettingsGroup, SettingsRow } from '@/ui/Settings'
import { ShortcutRecorder } from '@/ui/ShortcutRecorder'
import { Slider } from '@/ui/Slider'
import { Switch } from '@/ui/Switch'

export function OverlayPage() {
  const { overlayCfg, overlayVisible, toggleOverlay, updateOverlay, hotkeys, refreshHotkeys, setError } = useStore()

  const [draft, setDraft] = useState<OverlayConfig | null>(overlayCfg)
  useEffect(() => setDraft(overlayCfg), [overlayCfg])

  if (!overlayCfg || !draft) {
    return (
      <Page title="Окно субтитров">
        <p className="text-[var(--color-ink-2)] text-sm">Загрузка настроек…</p>
      </Page>
    )
  }

  async function rebind(id: string, accelerator: string) {
    try {
      await shortcutsSet(id, accelerator)
      await refreshHotkeys()
    } catch (e) {
      setError(formatCmdError(e))
    }
  }

  return (
    <Page title="Окно субтитров">
      <SettingsGroup title="Отображение">
        <SettingsRow
          icon="overlay"
          title={overlayVisible ? 'Скрыть субтитры' : 'Показать субтитры'}
          control={<Button onClick={() => void toggleOverlay()}>{overlayVisible ? 'Скрыть' : 'Показать'}</Button>}
        />
        <SettingsRow
          icon="keyboard"
          title="Режим взаимодействия"
          description={
            overlayCfg.mode === OverlayMode.Locked
              ? 'Клики проходят сквозь окно. Управление — горячими клавишами или через область уведомлений.'
              : 'Можно перемещать окно, менять его размер и выделять текст. Окно перехватывает клики.'
          }
          control={
            <Select
              label="Режим взаимодействия"
              value={overlayCfg.mode}
              options={[
                { value: OverlayMode.Locked, label: 'Заблокировано (клики насквозь)' },
                { value: OverlayMode.Edit, label: 'Редактирование' },
              ]}
              onChange={(v) => void overlaySetMode(v)}
            />
          }
        />
        <SettingsRow
          icon="check"
          title="Поверх всех окон"
          unavailableReason={isMacOS ? 'В macOS всегда включено.' : undefined}
          control={<span className="text-[var(--color-ink-2)] text-xs">Включено</span>}
        />
      </SettingsGroup>

      <SettingsGroup
        title="Горячие клавиши"
        footer="Используйте сочетание клавиши с Ctrl, Alt, Shift или ⌘ либо функциональную клавишу. Доступ к универсальному доступу в macOS не требуется."
      >
        {hotkeys.length === 0 ? (
          <SettingsRow title="Загрузка…" />
        ) : (
          hotkeys.map((h) => (
            <SettingsRow
              key={h.id}
              icon="keyboard"
              title={h.label}
              description={
                h.registered ? (
                  h.customized ? (
                    'Сочетание изменено'
                  ) : undefined
                ) : (
                  <span className="text-[var(--color-warn)]">
                    Не удалось назначить: возможно, сочетание занято другим приложением
                  </span>
                )
              }
              control={<ShortcutRecorder pretty={h.acceleratorPretty} onRecord={(accel) => void rebind(h.id, accel)} />}
            />
          ))
        )}
      </SettingsGroup>

      <SettingsGroup title="Внешний вид" footer="Прозрачность применяется только к фону; текст остаётся непрозрачным.">
        <SettingsRow
          icon="waveform"
          title="Размер текста"
          control={
            <div className="w-56">
              <Slider
                label="Размер текста"
                value={draft.fontSize}
                min={MIN_FONT_SIZE}
                max={MAX_FONT_SIZE}
                format={(v) => `${v} пкс`}
                onChange={(v) => setDraft((d) => (d ? { ...d, fontSize: v } : d))}
                onCommit={(v) => void updateOverlay({ fontSize: v })}
              />
            </div>
          }
        />
        <SettingsRow
          icon="overlay"
          title="Непрозрачность фона"
          description="Высокое значение закрывает изображение под окном."
          control={
            <div className="w-56">
              <Slider
                label="Непрозрачность фона"
                value={draft.opacity}
                min={0}
                max={100}
                format={(v) => `${v}%`}
                onChange={(v) => setDraft((d) => (d ? { ...d, opacity: v } : d))}
                onCommit={(v) => void updateOverlay({ opacity: v })}
              />
            </div>
          }
        />
        <SettingsRow
          icon="translate"
          title="Число строк"
          description="Остальные строки будут скрыты."
          control={
            <div className="w-56">
              <Slider
                label="Число строк"
                value={draft.maxLines}
                min={1}
                max={MAX_OVERLAY_LINES}
                format={(v) => `${v} стр.`}
                onChange={(v) => setDraft((d) => (d ? { ...d, maxLines: v } : d))}
                onCommit={(v) => void updateOverlay({ maxLines: v })}
              />
            </div>
          }
        />
        <SettingsRow
          icon="brush"
          title="Цвет текста"
          control={<ColorField value={overlayCfg.fontColor} onCommit={(v) => void updateOverlay({ fontColor: v })} />}
        />
        <SettingsRow
          icon="sparkle"
          title="Радужная рамка"
          control={
            <Switch
              label="Радужная рамка"
              checked={overlayCfg.glow}
              onChange={(v) => void updateOverlay({ glow: v })}
            />
          }
        />
        <SettingsRow
          icon="swatch"
          title="Цвет фона"
          control={<ColorField value={overlayCfg.bgColor} onCommit={(v) => void updateOverlay({ bgColor: v })} />}
        />
      </SettingsGroup>
    </Page>
  )
}

const COLOR_COMMIT_DELAY_MS = 150

function ColorField({ value, onCommit }: { value: string; onCommit: (v: string) => void }) {
  const [local, setLocal] = useState(value)
  const timer = useRef<number | null>(null)
  const latest = useRef(value)
  const commit = useRef(onCommit)

  useEffect(() => {
    commit.current = onCommit
  })

  useEffect(() => {
    if (timer.current === null) setLocal(value)
  }, [value])

  useEffect(
    () => () => {
      if (timer.current !== null) {
        window.clearTimeout(timer.current)
        commit.current(latest.current)
      }
    },
    [],
  )

  return (
    <div className="flex items-center gap-2">
      <input
        type="color"
        aria-label="Цвет"
        value={local}
        onChange={(e) => {
          const next = e.target.value
          latest.current = next
          setLocal(next)
          if (timer.current !== null) window.clearTimeout(timer.current)
          timer.current = window.setTimeout(() => {
            timer.current = null
            commit.current(latest.current)
          }, COLOR_COMMIT_DELAY_MS)
        }}
        className="h-6 w-12 cursor-pointer rounded-[var(--radius-ctl)] border border-[var(--color-border-subtle)] bg-transparent p-0"
      />
      <span className="font-mono text-[var(--color-ink-2)] text-xs">{local}</span>
    </div>
  )
}
