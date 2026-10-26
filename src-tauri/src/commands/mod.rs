use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

use crate::config::{
    direction_label, target_lang_for, EndpointConfig, EndpointProfile, OverlayConfig, OverlayMode,
    Preferences, PrefsPatch, SessionConfig, TranslateEngine,
};
use crate::{audio, overlay, permission, prefs, session, shortcut, update};

#[derive(Debug, thiserror::Error)]
pub enum CmdError {
    #[error("Неизвестная команда: {0}")]
    Unknown(String),

    #[error("Недопустимые параметры: {0}")]
    BadArgs(String),

    #[error("{message}")]
    Permission {
        permission: &'static str,
        message: String,
    },

    #[error("{0}")]
    Failed(String),
}

impl Serialize for CmdError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Permission {
                permission,
                message,
            } => json!({
                "kind": "permission",
                "permission": permission,
                "message": message,
            })
            .serialize(s),
            other => s.serialize_str(&other.to_string()),
        }
    }
}

#[tauri::command]
pub async fn dispatch(app: AppHandle, name: String, args: Value) -> Result<Value, CmdError> {
    tracing::debug!(cmd = %name, "Получена команда");
    let t0 = std::time::Instant::now();

    let out = route_async(&app, &name, &args).await;

    if let Err(e) = &out {
        tracing::warn!(cmd = %name, error = %e, "Не удалось выполнить команду");
    } else if name == "session_stop" {
        tracing::info!(ms = t0.elapsed().as_millis(), "Команда остановки выполнена");
    }

    out
}

async fn run_blocking<F, R>(task: F, timeout: std::time::Duration) -> Result<R, CmdError>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("cmd-blocking".into())
        .spawn(move || {
            let _ = tx.send(task());
        })
        .map_err(|e| CmdError::Failed(format!("Не удалось запустить фоновый поток: {e}")))?;

    await_task(rx, timeout).await
}

async fn await_task<R>(
    rx: tokio::sync::oneshot::Receiver<R>,
    timeout: std::time::Duration,
) -> Result<R, CmdError> {
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(_)) => Err(CmdError::Failed(
            "Фоновая задача неожиданно завершилась".into(),
        )),
        Err(_) => Err(CmdError::Failed(
            "Время ожидания фоновой задачи истекло. Повторите попытку".into(),
        )),
    }
}

type CmdTask = Box<dyn FnOnce() + Send>;

struct CmdExecutors {
    quick: std::sync::mpsc::Sender<CmdTask>,
    session: std::sync::mpsc::Sender<CmdTask>,
}

static CMD_EXECUTORS: std::sync::OnceLock<CmdExecutors> = std::sync::OnceLock::new();

fn spawn_cmd_lane(name: &str) -> Result<std::sync::mpsc::Sender<CmdTask>, String> {
    let (tx, rx) = std::sync::mpsc::channel::<CmdTask>();
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            while let Ok(task) = rx.recv() {
                task();
            }
        })
        .map_err(|e| format!("Не удалось запустить {name}: {e}"))?;
    Ok(tx)
}

pub fn init_executors() -> Result<(), String> {
    let quick = spawn_cmd_lane("cmd-quick")?;
    let session = spawn_cmd_lane("cmd-session")?;
    let _ = CMD_EXECUTORS.set(CmdExecutors { quick, session });
    Ok(())
}

#[derive(Clone, Copy)]
enum Lane {
    Quick,
    Session,
}

async fn run_lane<F, R>(lane: Lane, task: F, timeout: std::time::Duration) -> Result<R, CmdError>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let Some(executors) = CMD_EXECUTORS.get() else {
        return run_blocking(task, timeout).await;
    };

    let sender = match lane {
        Lane::Quick => &executors.quick,
        Lane::Session => &executors.session,
    };

    let (tx, rx) = tokio::sync::oneshot::channel();
    sender
        .send(Box::new(move || {
            let _ = tx.send(task());
        }))
        .map_err(|_| CmdError::Failed("Поток выполнения команд завершён".into()))?;

    await_task(rx, timeout).await
}

pub async fn route_async(app: &AppHandle, name: &str, args: &Value) -> Result<Value, CmdError> {
    match name {
        "update_check" => {
            return update::check(app)
                .await
                .map(|r| json!(r))
                .map_err(CmdError::Failed)
        }
        "update_install" => {
            return update::install(app)
                .await
                .map(|version| json!({ "version": version }))
                .map_err(CmdError::Failed)
        }
        _ => {}
    }

    let plan = match name {
        "session_start" => Some((Lane::Session, std::time::Duration::from_secs(60))),
        "permission_probe_system_audio" => Some((Lane::Quick, std::time::Duration::from_secs(30))),
        "permission_request_mic" => Some((Lane::Quick, std::time::Duration::from_secs(40))),
        "models_list" => Some((Lane::Quick, std::time::Duration::from_secs(10))),
        "model_remove" => Some((Lane::Quick, std::time::Duration::from_secs(60))),
        _ => None,
    };

    if let Some((lane, timeout)) = plan {
        let (app, name, args) = (app.clone(), name.to_string(), args.clone());
        return run_lane(lane, move || route(&app, &name, &args), timeout).await?;
    }

    route(app, name, args)
}

pub fn route(app: &AppHandle, name: &str, args: &Value) -> Result<Value, CmdError> {
    match name {
        "app_info" => Ok(app_info()),

        "update_restart" => {
            tracing::info!("Перезапуск для применения обновления");
            app.restart();
        }
        "overlay_toggle" => overlay::toggle(app)
            .map(|v| json!(v))
            .map_err(CmdError::Failed),
        "overlay_set_mode" => {
            let raw = str_arg(args, "mode")?;
            let mode = OverlayMode::parse(raw)
                .ok_or_else(|| CmdError::BadArgs(format!("Неизвестный режим: {raw}")))?;
            overlay::set_mode(app, mode)
                .map(|m| json!(m.as_str()))
                .map_err(CmdError::Failed)
        }

        "overlay_debug" => overlay::debug_state(app).map_err(CmdError::Failed),

        "prefs_get" => Ok(json!(prefs::load(app))),

        "prefs_set" => {
            let patch: PrefsPatch = json_arg(args, "patch")?;

            patch.validate().map_err(CmdError::BadArgs)?;

            prefs::update(app, |p| patch.apply_to(p))
                .map(|p| json!(p))
                .map_err(CmdError::Failed)
        }

        "prefs_reset" => {
            let fresh = Preferences::default();
            prefs::update(app, |p| *p = fresh.clone())
                .map(|_| json!(fresh))
                .map_err(CmdError::Failed)
        }

        "permission_status" => Ok(json!(permission::snapshot())),

        "permission_request_mic" => permission::request_mic()
            .map(|r| json!(r))
            .map_err(CmdError::Failed),

        "permission_probe_system_audio" => {
            if app.state::<session::SessionState>().is_running() {
                return Err(CmdError::Failed(
                    "Сначала остановите распознавание, затем запустите проверку".into(),
                ));
            }
            Ok(json!(permission::probe_system_audio(app)))
        }

        "open_privacy_settings" => {
            let pane = args
                .get("pane")
                .and_then(Value::as_str)
                .unwrap_or("systemAudio");
            let url = permission::settings_url(pane).ok_or_else(|| {
                CmdError::BadArgs(format!("Неизвестная страница настроек: {pane}"))
            })?;
            #[cfg(target_os = "macos")]
            if let Err(e) = std::process::Command::new("open").arg(url).spawn() {
                tracing::warn!(pane, "Не удалось открыть страницу системных настроек (возможно, изменилась схема URI): {e}");
            }
            Ok(json!(null))
        }

        "open_repo" => {
            open_with_system(std::ffi::OsStr::new(REPO_URL), "браузер")?;
            Ok(json!(null))
        }

        "open_sponsor" => {
            open_with_system(std::ffi::OsStr::new(SPONSOR_URL), "браузер")?;
            Ok(json!(null))
        }

        "open_url" => {
            let url = str_arg(args, "url")?;
            if !is_allowed_link(url) {
                return Err(CmdError::BadArgs(format!(
                    "Открытие сторонней ссылки запрещено: {url}"
                )));
            }
            open_with_system(std::ffi::OsStr::new(url), "ссылку")?;
            Ok(json!(null))
        }

        "reveal_path" => {
            let target = args
                .get("path")
                .and_then(Value::as_str)
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| app.state::<crate::log::LogDir>().0.clone());
            reveal_in_file_manager(&target)?;
            Ok(json!(null))
        }

        "logs_export" => {
            let dir = app.state::<crate::log::LogDir>().0.clone();
            let dest = export_logs(app, &dir)?;
            Ok(json!({ "path": dest.display().to_string() }))
        }

        "transcript_export" => {
            let stamp = args
                .get("stamp")
                .and_then(Value::as_str)
                .unwrap_or("export");
            let path = str_arg(args, "path")?;
            let path = export_transcript(app, stamp, std::path::Path::new(path))?;
            Ok(json!({ "path": path.display().to_string() }))
        }

        "autostart_get" => Ok(json!({
            "enabled": app.autolaunch().is_enabled().unwrap_or(false),
        })),

        "autostart_set" => {
            let on = args
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| CmdError::BadArgs("Отсутствует параметр enabled".into()))?;

            let r = if on {
                app.autolaunch().enable()
            } else {
                app.autolaunch().disable()
            };
            r.map_err(|e| CmdError::Failed(format!("Не удалось настроить автозапуск: {e}")))?;

            Ok(json!({
                "enabled": app.autolaunch().is_enabled().unwrap_or(false),
            }))
        }

        "models_list" => Ok(json!(crate::models::status::inspect(app))),

        "model_open_dir" => {
            let dir = crate::paths::ensure_user_models_root(app).map_err(CmdError::Failed)?;
            open_with_system(dir.as_os_str(), "папку моделей")?;
            Ok(json!(null))
        }

        "model_download" => {
            let id = str_arg(args, "modelId")?;
            let job_id =
                crate::models::install::start_download(app, id).map_err(model_cmd_error)?;
            Ok(json!({ "jobId": job_id }))
        }

        "model_cancel" => {
            let job_id = str_arg(args, "jobId")?;
            Ok(json!({ "cancelled": crate::models::install::cancel(app, job_id) }))
        }

        "model_remove" => {
            let id = str_arg(args, "modelId")?;
            crate::models::install::remove_model(app, id).map_err(model_cmd_error)?;
            Ok(json!(crate::models::status::inspect(app)))
        }

        "model_select" => {
            let id = str_arg(args, "modelId")?;
            Ok(json!(select_model(app, id)?))
        }

        "config_defaults" => Ok(json!({
            "overlay": OverlayConfig::default(),
            "endpoint": EndpointConfig::default(),
            "endpointProfiles": {
                "interpretation": EndpointConfig::for_profile(EndpointProfile::Interpretation),
                "dictation": EndpointConfig::for_profile(EndpointProfile::Dictation),
            },
        })),

        "overlay_config_get" => Ok(json!(overlay::config(app))),

        "overlay_visible" => Ok(json!(overlay::is_visible(app))),

        "overlay_config_set" => {
            let cfg: OverlayConfig = json_arg(args, "config")?;

            let applied = overlay::set_config(app, cfg).map_err(CmdError::Failed)?;
            Ok(json!(applied))
        }

        "audio_devices" => {
            let source = args
                .get("source")
                .and_then(Value::as_str)
                .and_then(audio::AudioSourceKind::parse)
                .unwrap_or_default();
            audio::list_devices(source)
                .map(|d| json!(d))
                .map_err(|e| CmdError::Failed(e.to_string()))
        }

        "session_start" => {
            let cfg: SessionConfig = args
                .get("config")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(|e| CmdError::BadArgs(format!("Не удалось прочитать конфигурацию: {e}")))?
                .unwrap_or_default();

            let device = args
                .get("device")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .filter(|s| !s.is_empty());

            let source = args
                .get("source")
                .and_then(Value::as_str)
                .and_then(audio::AudioSourceKind::parse)
                .unwrap_or_default();

            let mut cfg = cfg;
            cfg.asr.endpoint = prefs::load(app).endpoint.sanitized();

            tracing::info!(
                source_lang = %cfg.translate.source_lang,
                target_lang = %target_lang_for(&cfg.translate.source_lang),
                engine = cfg.translate.engine.as_str(),
                trailing_silence_ms = cfg.asr.endpoint.trailing_silence_ms,
                max_utterance_ms = cfg.asr.endpoint.max_utterance_ms,
                "Конфигурация сеанса прочитана; пропущенные поля заполнены значениями по умолчанию"
            );

            let device_for_prefs = device.clone();

            if let Some(permission) = permission::certain_denial(source) {
                return Err(CmdError::Permission {
                    permission,
                    message:
                        "Доступ к микрофону запрещён. Система больше не покажет запрос. Откройте системные настройки на странице «Разрешения» и предоставьте доступ."
                            .into(),
                });
            }

            match session::start(app, cfg, source, device) {
                Ok(()) => {
                    if let Err(e) =
                        prefs::update(app, |p| p.apply_session(source.as_str(), device_for_prefs))
                    {
                        tracing::error!("Не удалось сохранить настройки. Изменения будут потеряны после перезапуска: {e}");
                    }
                    Ok(json!(session::TaskStatus::Capturing))
                }
                Err(msg) => match permission::certain_denial(source) {
                    Some(permission) => Err(CmdError::Permission {
                        permission,
                        message: msg,
                    }),
                    None => Err(CmdError::Failed(msg)),
                },
            }
        }
        "session_stop" => session::stop(app)
            .map(|()| json!(session::TaskStatus::Ready))
            .map_err(CmdError::Failed),

        "session_snapshot" => Ok(json!(app.state::<session::SessionState>().snapshot())),

        "session_status" => Ok(json!(app.state::<session::SessionState>().info())),

        "session_clear" => {
            session::clear(app);
            Ok(json!(null))
        }

        "hotkey_failures" => Ok(json!(app.state::<shortcut::HotkeyState>().failures())),

        "shortcuts_get" => Ok(json!(shortcut::list(app))),

        "shortcuts_set" => {
            let id = str_arg(args, "id")?;
            let accelerator = str_arg(args, "accelerator")?;

            shortcut::rebind(app, id, accelerator)
                .map(|list| json!(list))
                .map_err(CmdError::Failed)
        }

        other => Err(CmdError::Unknown(other.to_string())),
    }
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, CmdError> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| CmdError::BadArgs(format!("Отсутствует параметр {key}")))
}

fn json_arg<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<T, CmdError> {
    let value = args
        .get(key)
        .ok_or_else(|| CmdError::BadArgs(format!("Отсутствует параметр {key}")))?;
    serde_json::from_value(value.clone())
        .map_err(|e| CmdError::BadArgs(format!("Не удалось прочитать {key}: {e}")))
}

fn app_info() -> Value {
    json!({
        "name": "AudioScript",
        "version": env!("CARGO_PKG_VERSION"),
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "builtAtMs": exe_mtime_ms(),
    })
}

const REPO_URL: &str = "https://github.com/envOnion/AudioScript";
const SITE_URL: &str = "https://github.com/envOnion/AudioScript#readme";

const SPONSOR_URL: &str = "https://ifdian.net/a/nico2026";

fn export_dir(app: &AppHandle) -> Result<std::path::PathBuf, CmdError> {
    app.path()
        .desktop_dir()
        .or_else(|_| app.path().home_dir())
        .map_err(|e| CmdError::Failed(format!("Не удалось найти доступную для записи папку: {e}")))
}

fn export_transcript(
    app: &AppHandle,
    stamp: &str,
    dest: &std::path::Path,
) -> Result<std::path::PathBuf, CmdError> {
    let segments = app.state::<session::SessionState>().snapshot();
    let p = prefs::load(app);
    let direction = match p.translate_engine {
        TranslateEngine::Off => None,
        _ => Some(direction_label(&p.source_lang)),
    };

    let mut file = dest.to_path_buf();
    if file.extension().is_none() {
        file.set_extension("md");
    }

    let md = crate::transcript::to_markdown(&segments, stamp, direction);
    std::fs::write(&file, md)
        .map_err(|e| CmdError::Failed(format!("Не удалось записать {}: {e}", file.display())))?;

    tracing::info!(path = %file.display(), "Расшифровка экспортирована");
    Ok(file)
}

fn is_allowed_link(url: &str) -> bool {
    const PREFIXES: [&str; 8] = [
        REPO_URL,
        SITE_URL,
        "https://github.com/niyongsheng/AudioScript",
        "https://github.com/k2-fsa/sherpa-onnx",
        "https://github.com/Helsinki-NLP/Opus-MT",
        "https://huggingface.co/Helsinki-NLP/",
        "https://www.apache.org/licenses/LICENSE-2.0",
        "https://creativecommons.org/licenses/by/4.0/",
    ];

    PREFIXES.iter().any(|prefix| {
        if prefix.ends_with('/') {
            url.starts_with(prefix)
        } else {
            url.strip_prefix(prefix)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        }
    })
}

fn open_with_system(target: &std::ffi::OsStr, what: &str) -> Result<(), CmdError> {
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut cmd = std::process::Command::new("explorer");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = std::process::Command::new("xdg-open");

    cmd.arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| CmdError::Failed(format!("Не удалось открыть {what}: {e}")))
}

fn reveal_in_file_manager(path: &std::path::Path) -> Result<(), CmdError> {
    if !path.exists() {
        return Err(CmdError::Failed(format!(
            "Путь не существует: {}",
            path.display()
        )));
    }
    open_with_system(path.as_os_str(), "файловый менеджер")
}

fn export_logs(app: &AppHandle, src: &std::path::Path) -> Result<std::path::PathBuf, CmdError> {
    let stamp = newest_log_stamp(src).unwrap_or_else(|| "unknown".into());
    let dest = export_dir(app)?.join(format!("AudioScript-журналы-{stamp}"));
    let files = copy_logs(src, &dest)?;

    tracing::info!(from = %src.display(), to = %dest.display(), files, "Журналы экспортированы");
    Ok(dest)
}

fn copy_logs(src: &std::path::Path, dest: &std::path::Path) -> Result<usize, CmdError> {
    std::fs::create_dir_all(dest)
        .map_err(|e| CmdError::Failed(format!("Не удалось создать {}: {e}", dest.display())))?;

    let entries = std::fs::read_dir(src)
        .map_err(|e| CmdError::Failed(format!("Не удалось прочитать папку журналов: {e}")))?;

    let mut copied = 0usize;
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let Some(name) = p.file_name() else { continue };
        std::fs::copy(&p, dest.join(name)).map_err(|e| {
            CmdError::Failed(format!("Не удалось скопировать {}: {e}", p.display()))
        })?;
        copied += 1;
    }

    if copied == 0 {
        return Err(CmdError::Failed(format!(
            "В папке {} нет файлов журнала для экспорта",
            src.display()
        )));
    }
    Ok(copied)
}

fn newest_log_stamp(dir: &std::path::Path) -> Option<String> {
    let mut stamps: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .filter_map(|n| {
            n.strip_prefix(crate::log::LOG_FILE_PREFIX)
                .and_then(|rest| rest.strip_prefix('.'))
                .map(str::to_owned)
        })
        .collect();
    stamps.sort();
    stamps.pop()
}

fn model_cmd_error(e: crate::models::ModelError) -> CmdError {
    use crate::models::ModelError as M;
    match e {
        M::UnknownModel(_)
        | M::NotDownloadable(_)
        | M::AlreadyInstalled(_)
        | M::Bundled
        | M::Selected
        | M::InUse
        | M::JobInProgress => CmdError::BadArgs(e.to_string()),
        _ => CmdError::Failed(e.to_string()),
    }
}

fn select_model(app: &AppHandle, id: &str) -> Result<crate::models::status::ModelsList, CmdError> {
    use crate::config::{AsrModelTier, MtModelTier};

    let entry = crate::models::catalog::entry(id)
        .ok_or_else(|| CmdError::BadArgs(format!("Неизвестная модель: {id}")))?;

    let mut list = crate::models::status::inspect(app);
    let complete = list
        .asr
        .iter()
        .chain(list.mt.iter())
        .find(|m| m.id == id)
        .map(|m| m.complete)
        .ok_or_else(|| CmdError::BadArgs(format!("Неизвестная модель: {id}")))?;
    if !complete {
        return Err(CmdError::BadArgs(format!(
            "Модель «{}» не установлена или её файлы повреждены. Сначала скачайте модель",
            entry.title
        )));
    }

    match entry.kind {
        crate::models::ModelKind::Asr => {
            let tier = AsrModelTier::parse(&entry.tier).ok_or_else(|| {
                CmdError::BadArgs(format!(
                    "Неизвестный вариант модели распознавания: {}",
                    entry.tier
                ))
            })?;
            crate::prefs::update(app, |p| p.asr_model_tier = tier).map_err(CmdError::Failed)?;
            for m in &mut list.asr {
                m.selected = m.id == id;
            }
        }
        crate::models::ModelKind::Mt => {
            let tier = MtModelTier::parse(&entry.tier).ok_or_else(|| {
                CmdError::BadArgs(format!(
                    "Неизвестный вариант модели перевода: {}",
                    entry.tier
                ))
            })?;
            crate::prefs::update(app, |p| p.mt_model_tier = tier).map_err(CmdError::Failed)?;
            for m in &mut list.mt {
                m.selected = m.id == id;
            }
        }
    }

    tracing::info!(model = %entry.id, "Модель выбрана; она будет использована при следующем запуске распознавания");
    Ok(list)
}

fn exe_mtime_ms() -> Option<u64> {
    let exe = std::env::current_exe().ok()?;
    let mtime = std::fs::metadata(exe).ok()?.modified().ok()?;
    mtime
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("audioscript-logs-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn copy_logs_copies_every_file() {
        let src = tmp("src");
        std::fs::write(src.join("audioscript.log.2026-10-15"), b"a").unwrap();
        std::fs::write(src.join("audioscript.log.2026-10-14"), b"b").unwrap();
        let dest = src.join("out");

        assert_eq!(copy_logs(&src, &dest).unwrap(), 2);
        assert!(dest.join("audioscript.log.2026-10-15").is_file());
        assert!(dest.join("audioscript.log.2026-10-14").is_file());

        std::fs::create_dir_all(src.join("sub")).unwrap();
        let dest2 = src.join("out2");
        assert_eq!(copy_logs(&src, &dest2).unwrap(), 2, "子目录不该被算进去");

        std::fs::remove_dir_all(&src).ok();
    }

    #[test]
    fn copy_logs_fails_on_empty_source_instead_of_succeeding_silently() {
        let src = tmp("empty");
        let e = copy_logs(&src, &src.join("out")).unwrap_err();
        assert!(e.to_string().contains("нет файлов журнала"), "{e}");
        std::fs::remove_dir_all(&src).ok();
    }

    #[test]
    fn newest_stamp_picks_the_latest_date() {
        let src = tmp("stamp");
        for d in ["2026-10-13", "2026-10-15", "2026-10-14"] {
            std::fs::write(src.join(format!("audioscript.log.{d}")), b"x").unwrap();
        }
        std::fs::write(src.join("notes.txt"), b"x").unwrap();

        assert_eq!(newest_log_stamp(&src).as_deref(), Some("2026-10-15"));
        std::fs::remove_dir_all(&src).ok();
    }

    #[test]
    fn newest_stamp_is_none_when_there_are_no_logs() {
        let src = tmp("nostamp");
        assert_eq!(newest_log_stamp(&src), None);
        std::fs::remove_dir_all(&src).ok();
    }

    #[test]
    fn only_known_links_can_be_opened() {
        assert!(is_allowed_link(
            "https://github.com/envOnion/AudioScript/releases/tag/v0.1.5"
        ));
        assert!(is_allowed_link("https://github.com/envOnion/AudioScript"));
        assert!(is_allowed_link(SITE_URL));
        assert!(is_allowed_link(
            "https://github.com/niyongsheng/AudioScript"
        ));
        assert!(!is_allowed_link(
            "https://github.com.evil/envOnion/AudioScript/"
        ));
        assert!(is_allowed_link(
            "https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models"
        ));
        assert!(is_allowed_link(
            "https://huggingface.co/Helsinki-NLP/opus-mt-en-ru"
        ));
        assert!(is_allowed_link(
            "https://creativecommons.org/licenses/by/4.0/"
        ));
        assert!(!is_allowed_link("https://github.com/envOnion/other"));
        assert!(!is_allowed_link(
            "https://github.com/envOnion/AudioScript-evil"
        ));
        assert!(!is_allowed_link(
            "https://github.com/k2-fsa/sherpa-onnx-evil"
        ));
        assert!(!is_allowed_link("https://huggingface.co/other/model"));
        assert!(!is_allowed_link("http://github.com/envOnion/AudioScript"));
        assert!(!is_allowed_link("javascript:alert(1)"));
    }
}
