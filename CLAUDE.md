# AudioScript: инструкции для разработки

Русский форк envOnion/AudioScript. Tauri 2, React 19, TypeScript, Rust, Bun 1.4.4. Интерфейс и пользовательские сообщения — на русском; языки распознавания и перевода — английский и русский. README описывает актуальные команды и модели; исходная docs/SPEC.md сохраняет архитектурные ограничения, но её китайские языковые настройки заменены en↔ru.

## Команды

```sh
bun install
bun run fetch:mt
bun tauri dev
bun run typecheck
bunx biome check src/
bun run build
bun tauri build
bun run verify:models --bundled
node scripts/check-localization.mjs
cargo test --manifest-path src-tauri/Cargo.toml
```

Реальные модельные проверки: `node scripts/fetch-canary.mjs`, затем `cargo test --manifest-path src-tauri/Cargo.toml bundled_ -- --ignored --nocapture --test-threads=1`.

## Ограничения

- Rust хранит истину состояния; frontend только отображает снапшоты и события. Не добавлять сторонний глобальный state manager.
- IPC проходит через `src/shared/ipc.ts` и `commands::route`. Блокирующие команды выполняются через заранее созданные `cmd-quick` / `cmd-session` потоки.
- Потоки перевода, отправки событий и остановки создаются при запуске. Не удалять прогрев CTranslate2: Windows может зависать при создании потоков во время инициализации MKL.
- `asr`, `translate`, `transcript`, `audio`, `config` не должны зависеть от Tauri.
- `src/shared/types.ts` генерируется typeshare из Rust; не править вручную. После изменения IPC: `bun run gen:types`, генератор typeshare-cli 1.13.4.
- `src/ui/Icon.tsx` — единая точка регистрации иконок. TS форматирует Biome. Rust форматировать только в изменённых файлах.
- Обе HTML-точки входа должны оставаться в Vite build. Обязательно проверять нативную сборку, а не только dev.
- Модели не коммитить. Новые файлы закреплять ревизией, размером и контрольной суммой. Соблюдать лицензии и атрибуцию.
- macOS разрешения аудио проверяются из `.app`, а не dev-бинарника. В GNOME Wayland возможности окон ограничены композитором.
- Обновления должны вести только в envOnion/AudioScript и проверяться ключом форка. Приватный ключ не выводить и не коммитить; он хранится локально; экспорт в GitHub Secret требует явного согласия владельца. Телеметрия отключена.
- CI создаёт черновик релиза; публиковать после проверки полного набора установщиков, подписей и SHA256SUMS.txt.
