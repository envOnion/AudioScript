import { save } from '@tauri-apps/plugin-dialog'
import { useStore } from '@/main/store'
import { formatCmdError, revealPath, transcriptExport } from '@/shared/ipc'
import { TaskStatus } from '@/shared/types'
import { useTranscriptStream } from '@/shared/useTranscriptStream'
import { Button } from '@/ui/Button'
import { Page } from '@/ui/Page'
import { SegmentList } from '@/ui/SegmentList'

function exportStamp(d = new Date()): string {
  return d.toLocaleString('ru-RU', { dateStyle: 'short', timeStyle: 'short' })
}

export function TranscribePage() {
  const { segments, clear } = useTranscriptStream()
  const { status, clearAll, setError } = useStore()
  const capturing = status === TaskStatus.Capturing
  const canExport = segments.some((s) => s.state !== 'partial')

  async function handleClear() {
    await clearAll()
    clear()
  }

  async function handleExport() {
    try {
      const stamp = exportStamp()
      const path = await save({
        defaultPath: `AudioScript-Расшифровка-${stamp.replace(/:/g, '-')}.md`,
        filters: [{ name: 'Markdown', extensions: ['md'] }],
      })
      if (!path) return
      const { path: saved } = await transcriptExport(stamp, path)
      await revealPath(saved)
    } catch (e) {
      setError(formatCmdError(e))
    }
  }

  return (
    <Page title="Расшифровка">
      <div className="flex items-center gap-2">
        <Button onClick={handleClear} disabled={capturing || segments.length === 0} size="sm">
          Очистить
        </Button>
        <Button icon="download" size="sm" disabled={!canExport} onClick={() => void handleExport()}>
          Экспорт
        </Button>
        <span className="ml-auto font-mono text-[var(--color-ink-2)] text-[0.7rem]">
          Фрагментов: {segments.length.toLocaleString('ru-RU')}
        </span>
      </div>

      <div className="max-h-[62vh] overflow-y-auto rounded-[var(--radius-panel)] border border-[var(--color-border-subtle)] bg-[var(--color-surface-0)] p-4">
        {segments.length === 0 ? (
          <p className="py-8 text-center text-[var(--color-ink-2)] text-sm">
            {capturing ? 'Распознавание…' : 'Нажмите «Начать запись» вверху слева.'}
          </p>
        ) : (
          <SegmentList segments={segments} />
        )}
      </div>

      {!capturing && segments.length > 0 && (
        <p className="text-[var(--color-ink-2)] text-[0.7rem]">
          После остановки текст сохранится до очистки или выхода из приложения.
        </p>
      )}
    </Page>
  )
}
