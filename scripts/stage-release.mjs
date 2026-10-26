import { copyFileSync, mkdirSync, readdirSync } from 'node:fs'
import { basename, join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const { version } = require('../package.json')
const [platform, target] = process.argv.slice(2)
if (!platform || !target) throw new Error('Нужны платформа и Rust target')
const root = join('src-tauri', 'target', target, 'release', 'bundle')
const out = join('.cache', 'release-assets', platform)
mkdirSync(out, { recursive: true })
function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const file = join(dir, entry.name)
    if (entry.name.endsWith('.app')) return []
    return entry.isDirectory() ? walk(file) : [file]
  })
}
let count = 0
for (const file of walk(root)) {
  if (!/\.(dmg|exe|msi|deb|AppImage|app\.tar\.gz)(\.sig)?$/.test(file)) continue
  let name = basename(file)
  if (name.startsWith('AudioScript.app.tar.gz')) {
    name = name.replace('AudioScript.app.tar.gz', `AudioScript_${version}_${target.split('-')[0]}.app.tar.gz`)
  }
  copyFileSync(file, join(out, name))
  console.log(name)
  count++
}
if (platform.startsWith('macOS') && !readdirSync(out).some((f) => f.endsWith('.app.tar.gz'))) {
  const archive = join(out, `AudioScript_${version}_${target.split('-')[0]}.app.tar.gz`)
  const result = spawnSync('tar', ['-czf', archive, '-C', join(root, 'macos'), 'AudioScript.app'], { stdio: 'inherit' })
  if (result.status !== 0) throw new Error('Не удалось создать пакет обновления macOS')
}
if (!count) throw new Error('Сборка не создала установочных файлов')
