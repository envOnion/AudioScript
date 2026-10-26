import { run } from '@tauri-apps/cli'

const args = process.argv.slice(2)

// Finder's DMG styling script waits forever when macOS does not write .DS_Store.
// Tauri's CI mode skips that cosmetic step while still producing the app and DMG.
if (process.platform === 'darwin' && ['build', 'bundle'].includes(args[0])) {
  process.env.CI = 'true'
}

try {
  await run(args, 'bun tauri')
} catch (error) {
  console.error(error.message)
  process.exitCode = 1
}
