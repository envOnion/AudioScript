import type { SidebarItem } from '@/ui/Sidebar'

export type PageId = 'transcribe' | 'audio' | 'permissions' | 'translate' | 'overlay' | 'general' | 'models' | 'about'

export const NAV: SidebarItem<PageId>[] = [
  {
    id: 'transcribe',
    label: 'Расшифровка',
    icon: 'waveform',
  },
  {
    id: 'audio',
    label: 'Аудио',
    icon: 'speaker',
  },
  { id: 'permissions', label: 'Разрешения', icon: 'shield' },
  {
    id: 'translate',
    label: 'Перевод',
    icon: 'translate',
  },
  {
    id: 'overlay',
    label: 'Окно субтитров',
    icon: 'overlay',
  },
  { id: 'general', label: 'Общие', icon: 'settings' },
  { id: 'models', label: 'Модели', icon: 'chip' },
  { id: 'about', label: 'О приложении', icon: 'info' },
]
