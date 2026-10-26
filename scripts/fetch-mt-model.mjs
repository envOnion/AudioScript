#!/usr/bin/env node

import { spawnSync } from 'node:child_process'
import { copyFile, mkdir, readFile, readdir, rm, stat } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const VENV = join(ROOT, '.cache/ct2venv')
const PY = join(VENV, process.platform === 'win32' ? 'Scripts/python.exe' : 'bin/python')
const catalog = JSON.parse(await readFile(join(ROOT, 'src-tauri/assets/model-catalog.json'), 'utf8'))
const model = catalog.models.find((entry) => entry.id === 'mt-base')
if (process.argv.includes('big')) throw new Error('Расширенный комплект перевода пока не опубликован')
const force = process.argv.includes('--force')
const MT_DIR = join(ROOT, 'src-tauri/assets/models/mt')

const CONVERT = `
import os, sys, json
from pathlib import Path
from huggingface_hub import snapshot_download
from ctranslate2.converters import TransformersConverter
from transformers import AutoTokenizer
import sentencepiece
from tokenizers import SentencePieceUnigramTokenizer

repo, revision, output, cache = sys.argv[1:]
source = snapshot_download(repo, revision=revision, cache_dir=cache,
    allow_patterns=['*.json', 'source.spm', 'target.spm', 'pytorch_model.bin', 'README.md', 'LICENSE*'])
TransformersConverter(source).convert(output, quantization='int8', force=True)
# Separate source/target SentencePiece vocabularies retain every piece and its original ID.
sys.path.insert(0, os.path.dirname(sentencepiece.__file__))
for side in ['source', 'target']:
    spm = str(Path(source) / (side + '.spm'))
    original = sentencepiece.SentencePieceProcessor(model_file=spm)
    converted = SentencePieceUnigramTokenizer.from_spm(spm)
    for i in range(original.get_piece_size()):
        assert converted.id_to_token(i) == original.id_to_piece(i), (side, i)
    converted.add_special_tokens(['<unk>', '<s>', '</s>', '<pad>'])
    probes = ['Hello, world!', 'This is a test of offline translation.', 'Привет, мир!', 'Это проверка перевода без интернета.']
    for text in probes:
        expected = original.encode(text, out_type=str)
        actual = converted.encode(text).tokens
        # Unknown characters are represented differently by the HF tokenizer.
        if original.unk_id() not in original.encode(text):
            assert actual == expected, (side, text, expected, actual)
    converted.save(str(Path(output) / (side + '-tokenizer.json')))
license_name = 'Apache-2.0' if repo.endswith('en-ru') else 'CC-BY-4.0'
license_url = 'https://www.apache.org/licenses/LICENSE-2.0' if license_name == 'Apache-2.0' else 'https://creativecommons.org/licenses/by/4.0/'
Path(output, 'NOTICE.txt').write_text('Helsinki-NLP / OPUS-MT\\nModel: ' + repo + '\\nRevision: ' + revision + '\\nSource: https://huggingface.co/' + repo + '\\nLicense: ' + license_name + ' ' + license_url + '\\nConverted by AudioScript to CTranslate2 int8; SentencePiece tokenizers exported to JSON.\\n', encoding='utf8')
Path(output, 'model-source.json').write_text(json.dumps({'repository': repo, 'revision': revision, 'conversion': 'ctranslate2-int8-marian-v2'}))
`

async function exists(path) {
  try { await stat(path); return true } catch { return false }
}
function run(command, args) {
  const result = spawnSync(command, args, { stdio: 'inherit' })
  if (result.error) throw result.error
  if (result.status !== 0) throw new Error(`${command}: код завершения ${result.status}`)
}
async function ensureVenv() {
  if (!await exists(PY)) {
    await mkdir(join(ROOT, '.cache'), { recursive: true })
    run(process.platform === 'win32' ? 'python' : 'python3', ['-m', 'venv', VENV])
  }
  const probe = spawnSync(PY, ['-c', 'import torch, ctranslate2, transformers, tokenizers, sentencepiece, google.protobuf; assert transformers.__version__ == "4.57.6"; assert ctranslate2.__version__ == "4.8.4"'], { stdio: 'ignore' })
  if (probe.status === 0) return
  process.stdout.write('Установка зависимостей конвертации моделей…\n')
  const torch = spawnSync(PY, ['-c', 'import torch'], { stdio: 'ignore' })
  if (torch.status !== 0) {
    run(PY, ['-m', 'pip', 'install', 'torch', ...(process.platform === 'linux' ? ['--index-url', 'https://download.pytorch.org/whl/cpu'] : [])])
  }
  run(PY, ['-m', 'pip', 'install', 'transformers==4.57.6', 'ctranslate2==4.8.4', 'sentencepiece==0.2.2', 'protobuf'])
}
async function dirSize(dir) {
  let total = 0
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    total += entry.isDirectory() ? await dirSize(path) : (await stat(path)).size
  }
  return total
}

for (const [index, source] of model.sources.entries()) {
  const output = join(MT_DIR, model.install.directions[index])
  const marker = join(output, 'model-source.json')
  let ready = false
  try {
    const provenance = JSON.parse(await readFile(marker, 'utf8'))
    ready = provenance.revision === source.revision && provenance.conversion === 'ctranslate2-int8-marian-v2' &&
      (await Promise.all(model.required.mt.files.map((file) => exists(join(output, file))))).every(Boolean)
  } catch {}
  if (!force && ready) continue
  await ensureVenv()
  await mkdir(MT_DIR, { recursive: true })
  await rm(output, { recursive: true, force: true })
  process.stdout.write(`⬇️ Конвертация ${source.repository} в CTranslate2 int8…\n`)
  run(PY, ['-c', CONVERT, source.repository, source.revision, output, join(ROOT, '.cache/mt-hf')])
}
for (const [index, source] of model.sources.entries()) {
  const license = source.repository.endsWith('en-ru') ? 'Apache-2.0' : 'CC-BY-4.0'
  await copyFile(join(ROOT, `src-tauri/assets/model-licenses/${license}.txt`), join(MT_DIR, model.install.directions[index], 'LICENSE.txt'))
}
process.stdout.write(`✅ Двунаправленный перевод готов: ${((await dirSize(MT_DIR)) / 1048576).toFixed(1)} МБ\n`)
