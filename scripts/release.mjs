#!/usr/bin/env node
import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')

const FILES = [
  'package.json',
  'src-tauri/Cargo.toml',
  'src-tauri/tauri.conf.json',
  'src-tauri/Cargo.lock',
]

function git(args) {
  return execFileSync('git', args, { cwd: ROOT, encoding: 'utf8' }).trim()
}

function gitInherit(args) {
  execFileSync('git', args, { cwd: ROOT, stdio: 'inherit' })
}

function run(cmd, args) {
  execFileSync(cmd, args, { cwd: ROOT, stdio: 'inherit' })
}

function fail(msg) {
  process.stderr.write(`\n❌ ${msg}\n`)
  process.exit(1)
}

function parse(version) {
  const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(version)
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null
}

function compare(a, b) {
  const [x, y] = [parse(a), parse(b)]
  for (let i = 0; i < 3; i += 1) {
    if (x[i] !== y[i]) return x[i] - y[i]
  }
  return 0
}

function bump(current, kind) {
  const [major, minor, patch] = parse(current)
  if (kind === 'major') return `${major + 1}.0.0`
  if (kind === 'minor') return `${major}.${minor + 1}.0`
  return `${major}.${minor}.${patch + 1}`
}

function read(rel) {
  return readFileSync(join(ROOT, rel), 'utf8')
}

function write(rel, text) {
  writeFileSync(join(ROOT, rel), text)
}

function readJsonVersion(rel) {
  return JSON.parse(read(rel)).version
}

const TOP_LEVEL_VERSION = /^(  "version": ")[^"]*(")/m

function bumpJsonVersion(rel, version) {
  const before = read(rel)
  const after = before.replace(TOP_LEVEL_VERSION, `$1${version}$2`)
  if (after === before) {
    fail(`${rel} 里没找到顶层 version 字段`)
  }
  write(rel, after)
}

function rewrite(version) {
  bumpJsonVersion('package.json', version)
  bumpJsonVersion('src-tauri/tauri.conf.json', version)

  const cargo = read('src-tauri/Cargo.toml')
  write('src-tauri/Cargo.toml', cargo.replace(/^version = "[^"]*"/m, `version = "${version}"`))

  const lock = read('src-tauri/Cargo.lock')
  const updated = lock.replace(
    /(\[\[package\]\]\nname = "audioscript"\nversion = ")[^"]*(")/,
    `$1${version}$2`,
  )
  if (updated === lock && !lock.includes(`name = "audioscript"\nversion = "${version}"`)) {
    fail('Cargo.lock 里没找到 audioscript 的版本条目')
  }
  write('src-tauri/Cargo.lock', updated)
}

function main() {
  const raw = process.argv[2]
  if (!raw) {
    fail('用法：bun run release <0.2.0 | patch | minor | major>')
  }

  const dirty = git(['status', '--porcelain'])
  if (dirty) {
    process.stderr.write(`${dirty}\n`)
    fail('工作区不干净，先提交或 stash 再发布')
  }

  const branch = git(['rev-parse', '--abbrev-ref', 'HEAD'])
  if (branch !== 'main') {
    fail(`当前在 ${branch}，发布必须在 main 上`)
  }

  const current = readJsonVersion('package.json')
  if (readJsonVersion('src-tauri/tauri.conf.json') !== current) {
    fail(`package.json 与 tauri.conf.json 的版本号已经不一致（${current}）`)
  }

  const version = raw.replace(/^v/, '')
  const isBump = ['patch', 'minor', 'major'].includes(version)
  if (!isBump && !parse(version)) {
    fail(`不是合法版本号：${raw}`)
  }
  const next = isBump ? bump(current, version) : version

  if (compare(next, current) < 0) {
    fail(`新版本 ${next} 不能低于当前版本 ${current}`)
  }

  const tag = `v${next}`
  if (git(['tag', '-l', tag])) {
    fail(`tag ${tag} 已存在`)
  }

  const rewriting = next !== current

  if (rewriting) {
    process.stdout.write(`\n🚀 发布 ${current} → ${next}\n\n`)
    process.stdout.write('📝 改写版本号…\n')
    rewrite(next)
    for (const f of FILES) {
      process.stdout.write(`   ${f}\n`)
    }

    try {
      process.stdout.write('\n🔍 跑检查…\n')
      run('bun', ['run', 'typecheck'])
      run('bun', ['run', 'lint'])
      run('cargo', ['test', '--manifest-path', 'src-tauri/Cargo.toml', '--quiet'])
    } catch {
      git(['checkout', '--', ...FILES])
      fail('检查未通过，版本号改动已还原')
    }

    process.stdout.write('\n📦 提交…\n')
    git(['add', ...FILES])
    git(['commit', '-m', `chore: release ${tag}`])
  } else {
    process.stdout.write(`\n🚀 为已存在的 ${current} 补打 tag（版本号未改动）\n`)
  }

  process.stdout.write(`📦 打 tag ${tag}…\n`)
  git(['tag', tag])

  process.stdout.write('⬆️  推送…\n')
  gitInherit(['push', 'origin', branch])
  gitInherit(['push', 'origin', tag])

  const remote = git(['remote', 'get-url', 'origin'])
    .replace(/^git@github\.com:/, 'https://github.com/')
    .replace(/\.git$/, '')

  process.stdout.write(`\n✅ ${tag} 已推送，CI 会自动构建并创建 Release\n`)
  process.stdout.write(`   ${remote}/releases/tag/${tag}\n\n`)
}

main()
