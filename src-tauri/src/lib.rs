pub mod asr;
pub mod audio;
pub mod commands;
pub mod config;
pub mod lifecycle;
pub mod log;
pub mod models;
mod native_menu;
pub mod overlay;
pub mod paths;
pub mod permission;
pub mod prefs;
pub mod session;
pub mod shortcut;
pub mod telemetry;
pub mod transcript;
pub mod translate;
pub mod tray;
pub mod update;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};

static MAIN_TICK_MS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn spawn_main_thread_watchdog(app: &AppHandle) {
    MAIN_TICK_MS.store(now_ms(), Ordering::Relaxed);
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("main-thread-watchdog".into())
        .spawn(move || {
            let mut last_warn = 0u64;
            loop {
                if app
                    .run_on_main_thread(|| {
                        MAIN_TICK_MS.store(now_ms(), Ordering::Relaxed);
                    })
                    .is_err()
                {
                    break;
                }

                std::thread::sleep(Duration::from_secs(2));
                let now = now_ms();
                let gap = now.saturating_sub(MAIN_TICK_MS.load(Ordering::Relaxed));
                if gap >= 5000 && now.saturating_sub(last_warn) >= 10_000 {
                    last_warn = now;
                    tracing::warn!(
                        gap_ms = gap,
                        "Главный поток не обрабатывал задачи более {} мс",
                        gap
                    );
                }
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("Не удалось запустить наблюдение за главным потоком: {e}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
        tracing::info!("Обнаружен повторный запуск. Открываем уже работающий экземпляр");
        lifecycle::show_main(app);
    }));

    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    builder
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(lifecycle::LifecycleState::default())
        .manage(overlay::OverlayState::default())
        .manage(shortcut::HotkeyState::default())
        .manage(models::state::ModelState::default())
        .manage(session::SessionState::default())
        .invoke_handler(tauri::generate_handler![commands::dispatch])
        .on_window_event(lifecycle::on_window_event)
        .on_page_load(|webview, payload| {
            if webview.label() != overlay::LABEL {
                return;
            }
            if !matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                return;
            }
            overlay::on_page_loaded(webview.app_handle());
        })
        .setup(|app| {
            let handle = app.handle().clone();

            handle.manage(log::LogDir(log::init(&handle)));

            if let Err(e) = native_menu::init(&handle) {
                tracing::error!("Не удалось инициализировать меню приложения: {e}");
            }

            if let Err(e) = session::init_emitter(&handle) {
                tracing::error!("{e}");
            }
            if let Err(e) = session::init_teardown_executor() {
                tracing::error!("{e}");
            }
            if let Err(e) = session::init_translate_worker() {
                tracing::error!("{e}");
            }
            if let Err(e) = commands::init_executors() {
                tracing::error!("{e}");
            }

            spawn_main_thread_watchdog(&handle);

            let cleanup_handle = handle.clone();
            if let Err(e) = std::thread::Builder::new()
                .name("models-cleanup".into())
                .spawn(move || {
                    paths::log_startup_selfcheck(&cleanup_handle);
                    models::install::cleanup_stale(&cleanup_handle);
                })
            {
                tracing::warn!("Не удалось запустить очистку временных папок моделей: {e}");
            }

            let saved_prefs = prefs::load(&handle);
            telemetry::track(
                &handle,
                "app_started",
                Some(serde_json::json!({
                    "asr_model_tier": saved_prefs.asr_model_tier.as_str(),
                    "mt_model_tier": saved_prefs.mt_model_tier.as_str(),
                    "translate_engine": saved_prefs.translate_engine.as_str(),
                    "source_lang": saved_prefs.source_lang,
                    "onboarding_done": if saved_prefs.onboarding_done { 1 } else { 0 },
                })),
            );
            if let Err(e) = overlay::init(&handle, saved_prefs.overlay, saved_prefs.overlay_geometry) {
                tracing::error!("Не удалось инициализировать окно субтитров: {e}");
            }

            if let Err(e) = tray::init(&handle) {
                tracing::error!("Не удалось инициализировать трей: {e}");
            }

            if let Err(e) = shortcut::init(&handle) {
                tracing::error!("Не удалось инициализировать глобальные горячие клавиши: {e}");
            }

            lifecycle::show_main(&handle);

            if std::env::var("AUDIOSCRIPT_SMOKE_ROUTE").is_ok() {
                let source = audio::source_from_env();
                let device = std::env::var(audio::ENV_DEVICE)
                    .ok()
                    .filter(|s| !s.is_empty());

                let args = serde_json::json!({
                    "device": device,
                    "source": source.as_str(),
                    "config": {
                        "translate": { "engine": "local-nmt", "sourceLang": "en" }
                    }
                });

                tracing::info!("Задана переменная AUDIOSCRIPT_SMOKE_ROUTE. Проверка запускается через обычный маршрут команд");

                for src in ["loopback", "mic"] {
                    match commands::route(
                        &handle,
                        "audio_devices",
                        &serde_json::json!({ "source": src }),
                    ) {
                        Ok(v) => {
                            let arr = v.as_array().cloned().unwrap_or_default();
                            let names: Vec<String> = arr
                                .iter()
                                .filter_map(|d| d["name"].as_str().map(str::to_owned))
                                .collect();
                            tracing::info!(
                                source = src,
                                count = arr.len(),
                                names = ?names,
                                "Проверка: список устройств получен"
                            );
                        }
                        Err(e) => {
                            tracing::error!(source = src, "Проверка: не удалось получить список устройств: {e}")
                        }
                    }
                }

                for (cmd, a) in [
                    ("app_info", serde_json::json!({})),
                    ("permission_status", serde_json::json!({})),
                    (
                        "open_privacy_settings",
                        serde_json::json!({ "pane": "несуществующая-страница" }),
                    ),
                    ("prefs_get", serde_json::json!({})),
                    ("config_defaults", serde_json::json!({})),
                    ("models_list", serde_json::json!({})),
                    (
                        "prefs_set",
                        serde_json::json!({ "patch": { "theme": "dark" } }),
                    ),
                    ("autostart_get", serde_json::json!({})),
                    ("autostart_set", serde_json::json!({ "enabled": false })),
                    ("logs_export", serde_json::json!({})),
                    ("shortcuts_get", serde_json::json!({})),
                    (
                        "shortcuts_set",
                        serde_json::json!({
                            "id": "overlay.toggleVisible",
                            "accelerator": "CmdOrControl+Shift+H"
                        }),
                    ),
                    (
                        "shortcuts_set",
                        serde_json::json!({ "id": "несуществующая-горячая-клавиша", "accelerator": "CmdOrControl+Shift+H" }),
                    ),
                ] {
                    match commands::route(&handle, cmd, &a) {
                        Ok(_) => tracing::info!(cmd, "Проверка: команда выполнена"),
                        Err(e) => tracing::error!(cmd, "Проверка: ошибка команды: {e}"),
                    }
                }

                let after = prefs::load(&handle);
                tracing::info!(
                    theme = ?after.theme,
                    audio_source = %after.audio_source,
                    source_lang = %after.source_lang,
                    font_size = after.overlay.font_size,
                    endpoint_ms = after.endpoint.trailing_silence_ms,
                    "Проверка: настройки после изменения theme (остальные поля должны остаться прежними)"
                );

                match commands::route(&handle, "session_start", &args) {
                    Ok(_) => tracing::info!("Проверка обычного маршрута команд: session_start выполнена"),
                    Err(e) => {
                        tracing::error!("Проверка обычного маршрута команд: ошибка session_start: {e}")
                    }
                }
            }

            if std::env::var("AUDIOSCRIPT_AUTOSTART").is_ok() {
                let device = std::env::var(audio::ENV_DEVICE)
                    .ok()
                    .filter(|s| !s.is_empty());
                let source = audio::source_from_env();
                let translate_source = std::env::var("AUDIOSCRIPT_SOURCE_LANG")
                    .ok()
                    .filter(|v| v == "zh" || v == "en")
                    .unwrap_or_else(|| "en".into());
                tracing::info!(
                    source = source.as_str(),
                    device = ?device,
                    "Задана переменная AUDIOSCRIPT_AUTOSTART. Распознавание запускается автоматически"
                );
                match session::start(
                    &handle,
                    config::SessionConfig {
                        asr: config::AsrConfig::default(),
                        translate: config::TranslateConfig {
                            engine: std::env::var("AUDIOSCRIPT_TRANSLATE")
                                .ok()
                                .and_then(|v| match v.as_str() {
                                    "off" => Some(config::TranslateEngine::Off),
                                    "mock" => Some(config::TranslateEngine::Mock),
                                    "local-nmt" => Some(config::TranslateEngine::LocalNmt),
                                    _ => None,
                                })
                                .unwrap_or(config::TranslateEngine::LocalNmt),
                            source_lang: translate_source.clone(),
                            ..Default::default()
                        },
                    },
                    source,
                    device,
                ) {
                    Ok(()) => tracing::info!("Автоматический запуск распознавания выполнен"),
                    Err(e) => tracing::error!("Не удалось автоматически запустить распознавание: {e}"),
                }
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Не удалось собрать приложение Tauri")
        .run(|app, event| match &event {
            tauri::RunEvent::ExitRequested { api, .. } => {
                if lifecycle::on_run_event(app, &event) {
                    api.prevent_exit();
                }
            }
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => lifecycle::show_main(app),
            tauri::RunEvent::Exit => {
                telemetry::track(app, "app_exited", None);
                telemetry::flush(app);
            }
            _ => {}
        });
}
