import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)])
}
const files = [...walk('src'), ...walk('src-tauri/src'), 'index.html', 'overlay.html', 'src-tauri/Info.plist', 'src-tauri/tauri.conf.json', 'src-tauri/assets/model-catalog.json']
const errors = []
for (const file of files) {
  if (file === 'src/shared/types.ts') continue
  let source = readFileSync(file, 'utf8')
  if (file.endsWith('.rs')) source = source.split('#[cfg(test)]')[0]
  source = source.replace(/\/\*[\s\S]*?\*\//g, '').replace(/<!--[\s\S]*?-->/g, '')
  for (const [index, line] of source.split('\n').entries()) {
    if (line.trim().startsWith('//')) continue
    if (/['"`].*\p{Script=Han}/u.test(line)) errors.push(`${file}:${index + 1}: ${line.trim()}`)
  }
}
for (const html of ['index.html', 'overlay.html']) {
  if (!readFileSync(html, 'utf8').includes('lang="ru"')) errors.push(`${html}: не указан русский язык документа`)
}
if (errors.length) throw new Error(`Найдены нелокализованные строки:\n${errors.join('\n')}`)
console.log('Русская локализация проверена: интерфейс, нативные сообщения и метаданные')
