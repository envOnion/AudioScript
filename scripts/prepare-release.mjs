import { createHash } from 'node:crypto'
import { createReadStream, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const dir = process.argv[2]
const version = process.env.RELEASE_TAG?.replace(/^v/, '')
const repository = process.env.RELEASE_REPOSITORY
if (!dir || !version || !repository) throw new Error('Не заданы параметры релиза')
const files = readdirSync(dir).sort()
const platforms = {}
const allowUnsigned = process.argv.includes('--allow-unsigned')
const targets = {
  'darwin-aarch64': /_aarch64\.app\.tar\.gz$/,
  'darwin-x86_64': /_x86_64\.app\.tar\.gz$/,
  'windows-x86_64': /-setup\.exe$/,
  'linux-x86_64': /\.AppImage$/,
}
for (const pattern of [/aarch64\.dmg$/, /x64\.dmg$/, /\.msi$/, /\.deb$/]) {
  if (!files.some((f) => pattern.test(f))) throw new Error(`Нет обязательного установщика: ${pattern}`)
}
for (const [target, pattern] of Object.entries(targets)) {
  const artifact = files.find((f) => pattern.test(f))
  if (!artifact) throw new Error(`Нет пакета обновления ${target}`)
  if (!files.includes(`${artifact}.sig`)) {
    if (allowUnsigned) continue
    throw new Error(`Нет подписи обновления ${target}`)
  }
  platforms[target] = {
    signature: readFileSync(join(dir, `${artifact}.sig`), 'utf8').trim(),
    url: `https://github.com/${repository}/releases/download/v${version}/${encodeURIComponent(artifact)}`,
  }
}
if (Object.keys(platforms).length === Object.keys(targets).length) writeFileSync(join(dir, 'latest.json'), `${JSON.stringify({ version, notes: 'Русская версия AudioScript: офлайн-перевод английский ↔ русский.', pub_date: new Date().toISOString(), platforms }, null, 2)}\n`)
const sums = []
for (const file of readdirSync(dir).sort()) {
  const hash = createHash('sha256')
  for await (const chunk of createReadStream(join(dir, file))) hash.update(chunk)
  sums.push(`${hash.digest('hex')}  ${file}`)
}
writeFileSync(join(dir, 'SHA256SUMS.txt'), `${sums.join('\n')}\n`)
console.log(`Готовы ${files.length} файлов, обновления для ${Object.keys(platforms).join(', ')}`)
