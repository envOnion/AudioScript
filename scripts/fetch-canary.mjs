import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
const catalog = JSON.parse(await readFile('src-tauri/assets/model-catalog.json', 'utf8'))
const sources = catalog.models.find((m) => m.id === 'asr-builtin').sources
await mkdir('.cache/asr-canary', { recursive: true })
for (const source of sources) {
  const base = `https://huggingface.co/${source.repository}/resolve/${source.revision}/test_wavs`
  const url = `${base}/0.wav`
  const response = await fetch(url)
  if (!response.ok) throw new Error(`${source.language}: HTTP ${response.status}`)
  await writeFile(join('.cache/asr-canary', `${source.language}.wav`), Buffer.from(await response.arrayBuffer()))
  if (source.language === 'en') {
    const transcript = await fetch(`${base}/trans.txt`)
    if (!transcript.ok) throw new Error(`Эталон en: HTTP ${transcript.status}`)
    await writeFile(join('.cache/asr-canary', 'en.txt'), await transcript.text())
  }
}
console.log('Официальные аудиопримеры готовы')
