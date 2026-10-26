import assert from 'node:assert/strict'
import { mkdtempSync, writeFileSync, readFileSync, rmSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
import test from 'node:test'

const files = ['AudioScript_0.3.1_aarch64.dmg', 'AudioScript_0.3.1_x64.dmg', 'AudioScript_0.3.1_aarch64.app.tar.gz', 'AudioScript_0.3.1_x86_64.app.tar.gz', 'AudioScript_0.3.1_x64-setup.exe', 'AudioScript_0.3.1_x64.msi', 'AudioScript_0.3.1_amd64.deb', 'AudioScript_0.3.1_amd64.AppImage']
function prepare(dir, args = []) {
  return spawnSync(process.execPath, [resolve('scripts/prepare-release.mjs'), dir, ...args], { encoding: 'utf8', env: { ...process.env, RELEASE_TAG: 'v0.3.1', RELEASE_REPOSITORY: 'envOnion/AudioScript' } })
}
test('release remains incomplete until every updater has a signature', () => {
  const dir = mkdtempSync(join(tmpdir(), 'audioscript-release-'))
  try {
    for (const file of files) writeFileSync(join(dir, file), `artifact:${file}`)
    assert.notEqual(prepare(dir).status, 0)
    assert.equal(prepare(dir, ['--allow-unsigned']).status, 0)
    assert.equal(existsSync(join(dir, 'latest.json')), false)
    for (const file of files.filter((f) => !f.endsWith('.dmg'))) writeFileSync(join(dir, `${file}.sig`), 'test-signature')
    assert.equal(prepare(dir).status, 0)
    const manifest = JSON.parse(readFileSync(join(dir, 'latest.json'), 'utf8'))
    assert.equal(Object.keys(manifest.platforms).length, 10)
    assert.match(manifest.platforms['windows-x86_64-msi'].url, /\.msi$/)
    assert.match(manifest.platforms['linux-x86_64-deb'].url, /\.deb$/)
    assert.match(manifest.platforms['darwin-x86_64-app'].url, /x86_64\.app\.tar\.gz$/)
    assert.ok(Object.values(manifest.platforms).every((p) => p.url.startsWith('https://github.com/envOnion/AudioScript/releases/download/v0.3.1/')))
    const sums = readFileSync(join(dir, 'SHA256SUMS.txt'), 'utf8')
    assert.match(sums, /^[a-f0-9]{64}  AudioScript_/)
    assert.doesNotMatch(sums, /  SHA256SUMS\.txt/)
  } finally { rmSync(dir, { recursive: true, force: true }) }
})
