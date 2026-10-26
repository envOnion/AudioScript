import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
const catalog = JSON.parse(await readFile('src-tauri/assets/model-catalog.json', 'utf8'))
const sources = catalog.models.find((m) => m.id === 'asr-builtin').sources
await mkdir('.cache/asr-canary', { recursive: true })
for (const source of sources) {
  const url = `https://huggingface.co/${source.repository}/resolve/${source.revision}/test_wavs/0.wav`
  const response = await fetch(url)
  if (!response.ok) throw new Error(`${source.language}: HTTP ${response.status}`)
  await writeFile(join('.cache/asr-canary', `${source.language}.wav`), Buffer.from(await response.arrayBuffer()))
}
console.log('Официальные аудиопримеры готовы')
