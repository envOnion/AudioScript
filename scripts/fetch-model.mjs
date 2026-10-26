#!/usr/bin/env node

import { createHash } from 'node:crypto'
import { createWriteStream } from 'node:fs'
import { copyFile, mkdir, readFile, rename, rm, stat, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { pipeline } from 'node:stream/promises'
import { Readable } from 'node:stream'

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const catalog = JSON.parse(await readFile(join(ROOT, 'src-tauri/assets/model-catalog.json'), 'utf8'))
if (process.argv.includes('high')) throw new Error('Дополнительный комплект ASR пока не опубликован')
const model = catalog.models.find((entry) => entry.id === 'asr-builtin')
const target = join(ROOT, 'src-tauri/assets/models', model.install.dir)
const force = process.argv.includes('--force')

async function valid(path, file) {
  try {
    const bytes = await readFile(path)
    if (bytes.length !== file.sizeBytes) return false
    if (file.sha256) return createHash('sha256').update(bytes).digest('hex') === file.sha256
    return createHash('sha1').update(`blob ${bytes.length}\0`).update(bytes).digest('hex') === file.gitBlobSha1
  } catch {
    return false
  }
}

for (const source of model.sources) {
  const dir = join(target, source.language)
  await mkdir(dir, { recursive: true })
  for (const file of source.files) {
    const path = join(dir, file.name)
    if (!force && await valid(path, file)) continue
    const url = `https://huggingface.co/${source.repository}/resolve/${source.revision}/${file.name}`
    process.stdout.write(`⬇️ ${source.language}: ${file.name}\n`)
    const response = await fetch(url)
    if (!response.ok) throw new Error(`Загрузка ${url}: HTTP ${response.status}`)
    const tmp = `${path}.tmp`
    try {
      await pipeline(Readable.fromWeb(response.body), createWriteStream(tmp))
      if (!await valid(tmp, file)) throw new Error(`Контрольная сумма не совпадает: ${file.name}`)
      await rename(tmp, path)
    } finally {
      await rm(tmp, { force: true })
    }
  }
  await copyFile(join(ROOT, 'src-tauri/assets/model-licenses/Apache-2.0.txt'), join(dir, 'LICENSE.txt'))
  await writeFile(join(dir, 'NOTICE.txt'), `Model: ${source.repository}\nRevision: ${source.revision}\nSource: https://huggingface.co/${source.repository}\nLicense: Apache-2.0 https://www.apache.org/licenses/LICENSE-2.0\nONNX conversion: k2-fsa/sherpa-onnx\n`)
}
const size = (await Promise.all(model.sources.flatMap((source) => source.files.map((file) => stat(join(target, source.language, file.name)))))).reduce((sum, file) => sum + file.size, 0)
process.stdout.write(`✅ Английский и русский ASR готовы: ${(size / 1048576).toFixed(1)} МБ\n`)
