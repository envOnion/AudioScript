import { useStore } from '@/main/store'
import { AudioSourceKind, openPrivacySettings } from '@/shared/ipc'
import { TaskStatus } from '@/shared/types'
import { Alert } from '@/ui/Alert'
import { Button } from '@/ui/Button'
import { ChoiceCard, ChoiceGroup } from '@/ui/ChoiceCard'
import { LevelMeter } from '@/ui/LevelMeter'
import { Page } from '@/ui/Page'
import { Select } from '@/ui/Select'
import { SectionLabel, SettingsGroup, SettingsRow } from '@/ui/Settings'

const SOURCE_OPTIONS = [
  {
    value: AudioSourceKind.Loopback,
    label: 'Звук системы',
    description: 'Распознаёт звук приложений. Требуется доступ к записи экрана и системного звука.',
  },
  { value: AudioSourceKind.Mic, label: 'Микрофон', description: 'Распознаёт только звук с микрофона.' },
  {
    value: AudioSourceKind.Mix,
    label: 'Микрофон + звук системы',
    description: 'Записывает голос собеседников и ваш голос. Подходит для встреч.',
  },
]

export function AudioPage({ onOpenPermissions }: { onOpenPermissions: () => void }) {
  const { audioSource, setAudioSource, devices, device, setDevice, deviceError, refreshDevices, audio, status } =
    useStore()

  const capturing = status === TaskStatus.Capturing
  const selected = devices.find((d) => d.id === device)

  return (
    <Page title="Аудио">
      {deviceError && (
        <Alert tone="warn" title="Не удалось получить список аудиоустройств">
          <p className="font-mono text-[0.95em]">{deviceError}</p>
          <p className="mt-1">Устройство по умолчанию доступно; выбрать другое устройство пока нельзя.</p>
          <p className="mt-1">
            Пока не выдан доступ к микрофону, macOS может скрывать весь список устройств. Проверьте раздел «Разрешения».
            <Button size="sm" className="ml-1.5" onClick={onOpenPermissions}>
              Разрешения
            </Button>
          </p>
        </Alert>
      )}

      {audio?.silent && audio.segments === 0 && (
        <SilenceHelp audioSource={audioSource} onOpenPermissions={onOpenPermissions} />
      )}

      <section className="space-y-2">
        <SectionLabel>Источник звука</SectionLabel>
        <ChoiceGroup label="Источник звука">
          {SOURCE_OPTIONS.map((o) => (
            <ChoiceCard
              key={o.value}
              title={o.label}
              description={o.description}
              selected={audioSource === o.value}
              disabled={capturing}
              onSelect={() => setAudioSource(o.value)}
            />
          ))}
        </ChoiceGroup>
        <p className="px-1 text-[var(--color-ink-2)] text-[0.7rem] leading-relaxed">
          При смене источника список устройств обновится.
        </p>
      </section>

      <SettingsGroup title="Устройство ввода">
        <SettingsRow
          icon="waveform"
          title="Устройство ввода"
          description={
            selected
              ? `${selected.channels} кан. · ${selected.sampleRate.toLocaleString('ru-RU')} Гц${selected.isDefault ? ' · По умолчанию' : ''}`
              : undefined
          }
          control={
            <div className="flex items-center gap-1.5">
              <LevelMeter active={capturing} />
              <Select
                label="Устройство ввода"
                value={device}
                options={
                  devices.length === 0
                    ? [{ value: '', label: '(Устройства не найдены)' }]
                    : devices.map((d) => ({
                        value: d.id,
                        label: d.name,
                      }))
                }
                onChange={setDevice}
                disabled={capturing}
                className="max-w-[13rem]"
              />
              <Button icon="refresh" size="sm" onClick={() => void refreshDevices()}>
                Обновить
              </Button>
            </div>
          }
        />
      </SettingsGroup>

      {capturing && (
        <p className="text-[var(--color-ink-2)] text-[0.7rem]">
          Новое устройство будет использовано при следующем запуске.
        </p>
      )}
    </Page>
  )
}

function SilenceHelp({
  audioSource,
  onOpenPermissions,
}: {
  audioSource: AudioSourceKind
  onOpenPermissions: () => void
}) {
  const systemAudio = audioSource !== AudioSourceKind.Mic
  const mic = audioSource !== AudioSourceKind.Loopback

  return (
    <Alert tone="warn" title="Нет звука (уровень ≈ 0)">
      <ul className="list-inside list-disc space-y-1">
        {systemAudio && (
          <>
            <li>Убедитесь, что воспроизведение идёт и системная громкость выше нуля.</li>
            <li>
              Проверьте доступ к записи экрана и системного звука в macOS.
              <span className="font-medium">При запрете доступа захват запускается, но получает только тишину</span>.
            </li>
            <li>Для виртуального устройства (например, BlackHole) сначала настройте передачу звука на него.</li>
          </>
        )}
        {mic && <li>Проверьте доступ к микрофону и выберите физический микрофон вместо виртуального устройства.</li>}
      </ul>
      <div className="mt-1.5 flex gap-1.5">
        <Button size="sm" onClick={onOpenPermissions}>
          Проверить разрешения
        </Button>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => {
            openPrivacySettings().catch(() => {})
          }}
        >
          Открыть настройки системы
        </Button>
      </div>
    </Alert>
  )
}
