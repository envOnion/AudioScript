<div align="center">
<img src="src-tauri/icons/128x128@2x.png" width="128" height="128" alt="AudioScript">
<h1>AudioScript</h1>
<p>Офлайн-субтитры и перевод английский ↔ русский</p>
<p><a href="https://github.com/envOnion/AudioScript/releases/latest">Скачать</a> · <a href="https://github.com/envOnion/AudioScript/issues">Сообщить о проблеме</a></p>
</div>

AudioScript слушает системный звук, микрофон или оба источника и показывает оригинал и перевод в плавающем окне. Подходит для видео, лекций, подкастов и разговоров. Распознавание и перевод выполняются на устройстве; после установки сеть для них не нужна. Телеметрия отключена.

Это русская версия [AudioScript от niyongsheng](https://github.com/niyongsheng/AudioScript), выпущенная под той же лицензией MIT.

## Возможности

- Русский интерфейс, трей, уведомления, разрешения и Markdown-экспорт.
- Два направления: **английский → русский** и **русский → английский**.
- Потоковое распознавание sherpa-onnx: отдельные встроенные модели английского и русского языка.
- Офлайн-перевод OPUS-MT / CTranslate2 с квантованием int8.
- Двуязычные субтитры поверх окон; настройка размера, цвета и прозрачности.
- Горячие клавиши, автозапуск, работа в трее и обновления из этого форка.

Распознанный текст появляется по мере речи. Перевод выполняется после завершения фразы — это субтитры с небольшой задержкой, зависящей от пауз, длины фразы и мощности компьютера.

## Установка

Артефакты доступны в [GitHub Releases](https://github.com/envOnion/AudioScript/releases/latest):

| Система | Файл |
|---|---|
| macOS 14.2+, Apple Silicon | `*_aarch64.dmg` |
| macOS 14.2+, Intel | `*_x64.dmg` |
| Windows 10/11, x64 | `*-setup.exe` или `*.msi` |
| Ubuntu 24.04+, x64 | `*.deb` или `*.AppImage` |

Для DEB: `sudo apt install ./AudioScript_*.deb`. Для AppImage: сделайте файл исполняемым (`chmod +x AudioScript_*.AppImage`) и запустите его. Для AppImage может понадобиться `libfuse2t64`.

В macOS разрешите запись системного звука; микрофон нужен только для соответствующего источника. Разрешения проверяются из установленного `.app`, поскольку dev-бинарник не имеет описаний разрешений в Info.plist. В GNOME Wayland возможности размещения поверх окон ограничены композитором.

Установщики не подписаны сертификатами Apple Developer / Windows Authenticode. Подпись обновлений Tauri проверяется отдельно; контрольные суммы загрузок находятся в `SHA256SUMS.txt`.

## Разработка с Bun

Нужны Bun 1.4.4, Node.js, Rust >= 1.90 и нативные инструменты платформы. В macOS: Xcode Command Line Tools и CMake (`xcode-select --install`, `brew install cmake`). Зависимости Ubuntu перечислены в `.github/workflows/build.yml`.

```bash
bun install
bun run fetch:mt
bun tauri dev
bun tauri build
```

`bun install` скачивает проверенные модели распознавания. `fetch:mt` при первом запуске создаёт Python-окружение, скачивает исходные веса и конвертирует их в CTranslate2. Нужен Python >= 3.10; первая подготовка может занять несколько минут. Модели и кэш не входят в Git.

```bash
bun run typecheck
bunx biome check src/
bun run verify:models
node scripts/check-localization.mjs
cargo test --manifest-path src-tauri/Cargo.toml
```

`src/shared/types.ts` генерируется из Rust и хранится в репозитории. После изменения IPC-типов установите `cargo install typeshare-cli --version 1.13.4 --locked`, добавьте `~/.cargo/bin` в PATH и выполните `bun run gen:types`. Ручные изменения этого файла не допускаются.

macOS-сборка использует CI-режим упаковщика Tauri: оформление окна DMG через Finder пропускается, чтобы сборка не зависала в ожидании `.DS_Store`. `.app` и DMG создаются. Для исходного оформления можно задать `TAURI_BUNDLER_DMG_IGNORE_CI=true`.

## Релизы

GitHub Actions готовит языковые модели один раз, затем собирает приложения на Windows, macOS ARM/Intel и Ubuntu. Теги `v*` создают черновик релиза с установщиками, подписанными пакетами обновления, `latest.json` и SHA-256. Публикация — после проверки полного набора файлов. Обновления подписываются собственным ключом форка. Ключ можно хранить локально и подписывать загруженные артефакты перед публикацией; в CI поддерживается отдельный GitHub Secret `TAURI_SIGNING_PRIVATE_KEY`. При отсутствии секрета CI создаёт черновик без подписей и манифеста обновлений; публиковать его до локального подписания нельзя.

## Лицензии и благодарности

[MIT](LICENSE). Исходный AudioScript — niyongsheng. Распознавание: [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx), английская Zipformer и русская Vosk Zipformer. Перевод: [Helsinki-NLP / OPUS-MT](https://huggingface.co/Helsinki-NLP), `opus-mt-en-ru` (Apache-2.0) и `opus-mt-ru-en` (CC-BY-4.0). Атрибуция моделей включена в каталог приложения и `NOTICE.txt` рядом с весами. Движки: CTranslate2, flexaudio, Tauri и tauri-nspanel.
