import { useState } from 'react'
import { TaskStatus } from '@/shared/types'
import { Alert } from '@/ui/Alert'
import { Button } from '@/ui/Button'
import { LevelStatus } from '@/ui/LevelStatus'
import { PrimaryAction } from '@/ui/PrimaryAction'
import { Sidebar } from '@/ui/Sidebar'
import { TitleBarSpacer } from '@/ui/TitleBarSpacer'
import { Brand } from './Brand'
import { NAV, type PageId } from './navigation'
import { AboutPage } from './pages/AboutPage'
import { AudioPage } from './pages/AudioPage'
import { GeneralPage } from './pages/GeneralPage'
import { ModelsPage } from './pages/ModelsPage'
import { OverlayPage } from './pages/OverlayPage'
import { PermissionsPage } from './pages/PermissionsPage'
import { TranscribePage } from './pages/TranscribePage'
import { TranslatePage } from './pages/TranslatePage'
import { StoreProvider, useStore } from './store'
import { WelcomeGate } from './WelcomeGate'

export function App() {
  return (
    <StoreProvider>
      <Root />
    </StoreProvider>
  )
}

function Root() {
  const { prefsReady, onboardingDone } = useStore()

  if (!prefsReady) return <BootSplash />
  return onboardingDone ? <Shell /> : <WelcomeGate />
}

function BootSplash() {
  return (
    <main className="flex h-full">
      <section className="flex min-w-0 flex-1 flex-col bg-[var(--color-surface-0)]">
        <TitleBarSpacer />
      </section>
    </main>
  )
}

function Shell() {
  const [page, setPage] = useState<PageId>('transcribe')
  const { status } = useStore()
  const capturing = status === TaskStatus.Capturing

  return (
    <main className="flex h-full">
      <Sidebar items={NAV} current={page} onNavigate={setPage} brand={<Brand />} header={<PrimaryActionSlot />} />

      <section className="flex min-w-0 flex-1 flex-col bg-[var(--color-surface-0)]">
        <TitleBarSpacer />

        <Alerts page={page} onOpenPermissions={() => setPage('permissions')} onOpenAbout={() => setPage('about')} />

        <div className="flex-1 overflow-y-auto">
          <PageSwitch page={page} onOpenPermissions={() => setPage('permissions')} />
        </div>

        <footer className="border-[var(--color-border-subtle)] border-t py-2">
          <div className="flex items-center px-6">
            <LevelStatus active={capturing} />
          </div>
        </footer>
      </section>
    </main>
  )
}

function PrimaryActionSlot() {
  const { status, pending, activeDevice, toggleSession } = useStore()
  return (
    <PrimaryAction
      capturing={status === TaskStatus.Capturing}
      pending={pending}
      activeDevice={activeDevice}
      onToggle={() => void toggleSession()}
    />
  )
}

interface PageSwitchProps {
  page: PageId
  onOpenPermissions: () => void
}

function PageSwitch({ page, onOpenPermissions }: PageSwitchProps) {
  switch (page) {
    case 'transcribe':
      return <TranscribePage />
    case 'audio':
      return <AudioPage onOpenPermissions={onOpenPermissions} />
    case 'permissions':
      return <PermissionsPage />
    case 'translate':
      return <TranslatePage />
    case 'overlay':
      return <OverlayPage />
    case 'general':
      return <GeneralPage />
    case 'models':
      return <ModelsPage />
    case 'about':
      return <AboutPage />
  }
}

interface AlertsProps {
  page: PageId
  onOpenPermissions: () => void
  onOpenAbout: () => void
}

function Alerts({ page, onOpenPermissions, onOpenAbout }: AlertsProps) {
  const {
    error,
    setError,
    hotkeyProblems,
    audio,
    status,
    permissionError,
    setPermissionError,
    updateInfo,
    updatePhase,
  } = useStore()

  return (
    <>
      {updateInfo?.available && page !== 'about' && updatePhase !== 'downloading' && updatePhase !== 'ready' && (
        <Alert tone="info" title={`Доступна новая версия v${updateInfo.version ?? ''}`}>
          <Button size="sm" onClick={onOpenAbout}>
            Открыть «О приложении»
          </Button>
        </Alert>
      )}

      {error && (
        <Alert tone="error" title="Не удалось выполнить действие">
          <p className="font-mono">{error}</p>
          <button type="button" onClick={() => setError(null)} className="mt-1 underline opacity-70 hover:opacity-100">
            Закрыть
          </button>
        </Alert>
      )}

      {permissionError && (
        <Alert tone="warn" title="Требуются системные разрешения">
          <p>{permissionError.message}</p>
          <div className="mt-1.5 flex gap-1.5">
            <Button size="sm" onClick={onOpenPermissions}>
              Открыть «Разрешения»
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setPermissionError(null)}>
              Закрыть
            </Button>
          </div>
        </Alert>
      )}

      {hotkeyProblems.length > 0 && (
        <Alert tone="warn" title={`${hotkeyProblems.length} горячих клавиш не удалось назначить`}>
          {hotkeyProblems.map((h) => `${h.accelerator} ${h.label}`).join(', ')}. Назначьте сочетания заново в разделе
          «Окно субтитров».
        </Alert>
      )}

      {audio?.silent && audio.segments === 0 && status === TaskStatus.Capturing && (
        <Alert tone="warn" title="Нет звука (уровень ≈ 0)">
          Инструкции по проверке находятся в разделе «Аудио».
        </Alert>
      )}
    </>
  )
}
