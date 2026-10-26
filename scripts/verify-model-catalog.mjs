#!/usr/bin/env node

import { createHash } from 'node:crypto'
import { readFile, stat } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const catalog = JSON.parse(await readFile(join(ROOT, 'src-tauri/assets/model-catalog.json'), 'utf8'))
const online = process.argv.includes('--online')
const bundled = process.argv.includes('--bundled')
const errors = []
const check = (condition, message) => { if (!condition) errors.push(message) }
const safeRel = (path) => typeof path === 'string' && path.length > 0 && path.split('/').every((part) => /^[A-Za-z0-9._-]+$/.test(part) && part !== '..' && part !== '.')
check(catalog.version === 1, `Неподдерживаемая версия каталога: ${catalog.version}`)
const ids = new Set()
for (const model of catalog.models) {
  check(!ids.has(model.id), `Повторяющийся идентификатор: ${model.id}`)
  ids.add(model.id)
  check(safeRel(model.install.dir), `Небезопасный каталог: ${model.install.dir}`)
  for (const dir of model.install.directions || []) check(safeRel(dir), `Небезопасный каталог направления: ${dir}`)
  if (model.downloadable) {
    check(model.archive?.url.startsWith('https://'), `Отсутствует HTTPS-архив: ${model.id}`)
    check(/^[0-9a-f]{64}$/i.test(model.archive?.sha256), `Отсутствует SHA-256: ${model.id}`)
    check(model.archive?.sizeBytes > 0 && model.archive?.maxExtractedBytes > 0, `Отсутствует размер архива: ${model.id}`)
  }
  for (const source of model.sources || []) {
    check(/^[0-9a-f]{40}$/.test(source.revision), `Не закреплена ревизия: ${source.repository}`)
    for (const file of source.files || []) {
      check(safeRel(file.name), `Небезопасное имя файла: ${file.name}`)
      check(file.sizeBytes > 0, `Отсутствует размер: ${file.name}`)
      check(/^[0-9a-f]{64}$/.test(file.sha256) || /^[0-9a-f]{40}$/.test(file.gitBlobSha1), `Отсутствует контрольная сумма: ${file.name}`)
      if (!bundled && !online) continue
      const url = `https://huggingface.co/${source.repository}/resolve/${source.revision}/${file.name}`
      let bytes
      if (online) {
        const response = await fetch(url)
        if (!response.ok) { errors.push(`Загрузка ${url}: HTTP ${response.status}`); continue }
        bytes = Buffer.from(await response.arrayBuffer())
      } else {
        try { bytes = await readFile(join(ROOT, 'src-tauri/assets/models', model.install.dir, source.language, file.name)) }
        catch { errors.push(`Отсутствует встроенный файл: ${source.language}/${file.name}`); continue }
      }
      check(bytes.length === file.sizeBytes, `Размер не совпадает: ${file.name}`)
      const actual = file.sha256 ? createHash('sha256').update(bytes).digest('hex') : createHash('sha1').update(`blob ${bytes.length}\0`).update(bytes).digest('hex')
      check(actual === (file.sha256 || file.gitBlobSha1), `Контрольная сумма не совпадает: ${file.name}`)
    }
  }
  if (bundled && model.bundled) {
    const dirs = model.kind === 'asr' ? model.sources.map((source) => source.language) : model.install.directions
    for (const dir of dirs) {
      for (const file of ['NOTICE.txt', 'LICENSE.txt']) {
        try { check((await stat(join(ROOT, 'src-tauri/assets/models', model.install.dir, dir, file))).size > 0, `Пустой файл лицензии: ${dir}/${file}`) }
        catch { errors.push(`Отсутствует файл лицензии: ${dir}/${file}`) }
      }
    }
  }
  if (bundled && model.kind === 'mt' && model.bundled) {
    for (const [index, dir] of model.install.directions.entries()) {
      const root = join(ROOT, 'src-tauri/assets/models', model.install.dir, dir)
      for (const file of model.required.mt.files) {
        try { check((await stat(join(root, file))).size > 0, `Пустой файл: ${dir}/${file}`) }
        catch { errors.push(`Отсутствует встроенный файл: ${dir}/${file}`) }
      }
      try {
        const provenance = JSON.parse(await readFile(join(root, 'model-source.json'), 'utf8'))
        check(provenance.revision === model.sources[index].revision && provenance.conversion === 'ctranslate2-int8-marian-v2', `Неверная ревизия перевода: ${dir}`)
      } catch { errors.push(`Отсутствует происхождение модели: ${dir}`) }
    }
  }
}
for (const [kind, tier] of [['asr', 'builtin'], ['asr', 'high-accuracy'], ['mt', 'base'], ['mt', 'big']]) {
  check(catalog.models.filter((model) => model.kind === kind && model.tier === tier).length === 1, `Для ${kind}/${tier} требуется ровно одна запись`)
}
if (errors.length) { for (const error of errors) console.error(`❌ ${error}`); process.exit(1) }
process.stdout.write(`✅ Каталог моделей проверен${bundled ? ', включая встроенные файлы' : ''}${online ? ', включая загрузку исходных файлов' : ''}\n`)
