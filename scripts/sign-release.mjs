import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { run } from '@tauri-apps/cli'

const [dir, keyPath] = process.argv.slice(2)
const { version } = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'))
if (!dir || !keyPath) throw new Error('Использование: node scripts/sign-release.mjs каталог путь-к-локальному-ключу')
process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ''
for (const file of readdirSync(dir)) {
  if (!/\.(exe|msi|deb|AppImage|app\.tar\.gz)$/.test(file)) continue
  await run(['signer', 'sign', '--private-key-path', keyPath, '--app-version', version, join(dir, file)], 'bun tauri')
}
