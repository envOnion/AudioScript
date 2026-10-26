import { useEffect, useRef, useState } from 'react'
import { EVENT, on } from '@/shared/ipc'
import { cn } from './cn'

const SEGMENTS = [0, 1, 2, 3, 4] as const

const FLOOR_DB = -60
const CEIL_DB = -6

const FALL = 0.75

function levelRatio(value: number): number {
  const db = 20 * Math.log10(Math.max(value, 1e-9))
  return Math.min(1, Math.max(0, (db - FLOOR_DB) / (CEIL_DB - FLOOR_DB)))
}

function Dots({ dots, animated }: { dots: number; animated?: boolean }) {
  return (
    <div className="flex items-center gap-1" role="img" aria-label="Уровень входного сигнала">
      {SEGMENTS.map((seg) => (
        <span
          key={seg}
          className={cn(
            'size-1.5 rounded-full',
            animated && 'transition-colors duration-150',
            seg < dots ? 'bg-[var(--color-success)]' : 'bg-[var(--color-surface-2)]',
          )}
        />
      ))}
    </div>
  )
}

export function LevelBars({ level }: { level: number }) {
  return <Dots dots={Math.ceil(levelRatio(level) * SEGMENTS.length)} />
}

export function LevelMeter({ active }: { active: boolean }) {
  const [dots, setDots] = useState(0)
  const peak = useRef(0)

  useEffect(() => {
    if (!active) {
      peak.current = 0
      setDots(0)
      return
    }

    let disposed = false
    let unlisten: (() => void) | undefined

    on<number>(EVENT.level, (rms) => {
      const target = levelRatio(rms)
      peak.current = Math.max(target, peak.current * FALL)
      setDots(Math.ceil(peak.current * SEGMENTS.length))
    }).then((fn) => {
      if (disposed) fn()
      else unlisten = fn
    })

    return () => {
      disposed = true
      unlisten?.()
    }
  }, [active])

  return <Dots dots={dots} animated />
}
