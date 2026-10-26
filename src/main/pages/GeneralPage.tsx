import { useEffect, useState } from 'react'
import { useStore } from '@/main/store'
import { autostartGet, autostartSet, formatCmdError, logsExport, prefsReset, revealPath, Theme } from '@/shared/ipc'
import { Alert } from '@/ui/Alert'
import { Button } from '@/ui/Button'
import { Page } from '@/ui/Page'
import { Select } from '@/ui/Select'
import { SettingsGroup, SettingsRow } from '@/ui/Settings'
import { Switch } from '@/ui/Switch'

const THEME_OPTIONS = [
  { value: Theme.System, label: 'Как в системе' },
  { value: Theme.Light, label: 'Светлая' },
  { value: Theme.Dark, label: 'Тёмная' },
]

export function GeneralPage() {
  const { theme, changeTheme, setError } = useStore()

  const [autostart, setAutostart] = useState<boolean | null>(null)
  const [confirmingReset, setConfirmingReset] = useState(false)
  const [busy, setBusy] = useState(false)
  const [note, setNote] = useState<string | null>(null)

  useEffect(() => {
    autostartGet()
      .then((s) => setAutostart(s.enabled))
      .catch(() => setAutostart(null))
  }, [])

  async function toggleAutostart(next: boolean) {
    try {
      const s = await autostartSet(next)
      setAutostart(s.enabled)
    } catch (e) {
      setError(formatCmdError(e))
    }
  }

  async function doReset() {
    setBusy(true)
    setNote(null)
    try {
      await prefsReset()
      setNote('Настройки сброшены. Для некоторых изменений нужен перезапуск приложения.')
    } catch (e) {
      setError(formatCmdError(e))
    } finally {
      setBusy(false)
      setConfirmingReset(false)
    }
  }

  async function doExportLogs() {
    setBusy(true)
    setNote(null)
    try {
      const { path } = await logsExport()
      await revealPath(path)
      setNote(`Журнал сохранён в ${path}`)
    } catch (e) {
      setError(formatCmdError(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Page title="Общие">
      {note && <Alert tone="info">{note}</Alert>}

      <SettingsGroup title="Внешний вид">
        <SettingsRow
          icon="settings"
          title="Тема"
          description="Окно субтитров всегда использует тёмную тему."
          control={<Select label="Тема" value={theme} options={THEME_OPTIONS} onChange={(v) => void changeTheme(v)} />}
        />
      </SettingsGroup>

      <SettingsGroup title="Запуск">
        <SettingsRow
          icon="play"
          title="Запускать при входе"
          description="После закрытия главного окна приложение остаётся в области уведомлений."
          control={
            <Switch
              label="Запускать при входе"
              checked={autostart === true}
              disabled={autostart === null}
              onChange={(v) => void toggleAutostart(v)}
            />
          }
        />
      </SettingsGroup>

      <SettingsGroup title="Конфиденциальность">
        <SettingsRow
          icon="shield"
          title="Обработка на устройстве"
          description="Аудио и тексты обрабатываются на устройстве. Телеметрия отключена."
        />
      </SettingsGroup>

      <SettingsGroup title="Диагностика">
        <SettingsRow
          icon="folder"
          title="Экспорт журнала"
          description="Сохраняет журнал на рабочий стол с датой в имени и открывает его папку."
          control={
            <Button icon="external" size="sm" disabled={busy} onClick={() => void doExportLogs()}>
              Экспорт
            </Button>
          }
        />
        <SettingsRow
          icon="folder"
          title="Открыть папку журналов"
          description="Открывает папку с исходными журналами."
          onClick={() => void revealPath().catch((e) => setError(formatCmdError(e)))}
        />
      </SettingsGroup>

      <SettingsGroup title="Управление настройками">
        <SettingsRow
          icon="reset"
          title="Сбросить все настройки"
          description="Все настройки будут удалены без возможности отмены. При следующем запуске откроется приветствие."
          control={
            confirmingReset ? (
              <div className="flex gap-1.5">
                <Button variant="danger" size="sm" disabled={busy} onClick={() => void doReset()}>
                  Подтвердить сброс
                </Button>
                <Button size="sm" onClick={() => setConfirmingReset(false)}>
                  Отмена
                </Button>
              </div>
            ) : (
              <Button variant="danger" size="sm" onClick={() => setConfirmingReset(true)}>
                Сбросить
              </Button>
            )
          }
        />
      </SettingsGroup>
    </Page>
  )
}
