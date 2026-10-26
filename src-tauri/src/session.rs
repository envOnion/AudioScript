use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::asr::{AsrEngine, TARGET_SAMPLE_RATE};
use crate::audio::{self, CaptureHandle, SILENCE_RMS};
use crate::config::{
    target_lang_for, AsrConfig, EngineType, MtModelTier, SessionConfig, TranslateConfig,
    TranslateEngine,
};
use crate::models::catalog;
use crate::paths;
use crate::transcript::{TranscriptSegment, TranscriptStore, TranslationState};
use crate::translate::queue::{Job, JobQueue};
use crate::translate::Translator;

const TRANSLATE_QUEUE_CAPACITY: usize = 32;

fn create_translator(
    app: &AppHandle,
    cfg: &TranslateConfig,
    mt_tier: MtModelTier,
) -> Result<Box<dyn Translator>, String> {
    let engine: Box<dyn Translator> = match cfg.engine {
        TranslateEngine::LocalNmt => {
            let roots = paths::search_roots(app);
            let resolved = paths::locate_mt_root(&roots, mt_tier)?;
            tracing::info!(
                tier = mt_tier.as_str(),
                via = resolved.via.as_str(),
                path = %resolved.path.display(),
                "Каталог модели перевода"
            );
            let missing = paths::missing_mt_files(&resolved.path, catalog::mt_entry(mt_tier))?;
            if !missing.is_empty() {
                return Err(format!(
                    "Модель перевода «{}» не установлена или повреждена: {}\nОтсутствуют: {}\nВыберите встроенный перевод на странице «Модели»",
                    catalog::mt_entry(mt_tier).title,
                    resolved.path.display(),
                    missing.join(", ")
                ));
            }
            Box::new(crate::translate::local_nmt::LocalNmt::new(
                resolved.path,
                mt_tier,
            ))
        }
        TranslateEngine::Mock => {
            let fail_every = std::env::var(crate::translate::mock::ENV_FAIL_EVERY)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            Box::new(crate::translate::mock::MockTranslator::new(
                crate::translate::mock::DEFAULT_DELAY,
                fail_every,
            ))
        }
        TranslateEngine::Off => Box::new(crate::translate::mock::MockTranslator::new(
            std::time::Duration::ZERO,
            0,
        )),
        TranslateEngine::RemoteApi => {
            return Err(format!(
                "Движок перевода {} пока не поддерживается. Доступны local-nmt (локальный), mock (для разработки), off (только оригинал)",
                cfg.engine.as_str()
            ));
        }
    };

    Ok(engine)
}

pub const EVENT_SEGMENTS: &str = "transcript://segments";
pub const EVENT_TASK_STATE: &str = "transcript://state";
pub const EVENT_AUDIO: &str = "transcript://audio";
pub const EVENT_LEVEL: &str = "transcript://level";

const FIRST_TEXT_BUDGET_MS: u128 = 200;

#[typeshare::typeshare]
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioStatus {
    pub silent: bool,
    pub rms: f64,
    pub segments: u32,
}

const EMIT_INTERVAL: Duration = Duration::from_millis(100);

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

const LEVEL_INTERVAL: Duration = Duration::from_millis(200);

#[typeshare::typeshare]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    #[default]
    Ready,
    Capturing,
    Error,
}

struct RunningSession {
    capture: CaptureHandle,
    engine: Arc<Mutex<Box<dyn AsrEngine>>>,
    device: String,
    device_sample_rate: u32,

    queue: Arc<JobQueue>,
    translator: Arc<Mutex<Box<dyn Translator>>>,
    events: Sender<UiEvent>,
}

enum UiEvent {
    Status(TaskStatus),
    Level(f64),
    Audio(AudioStatus),
    Segments(Vec<TranscriptSegment>),
}

static EVENT_SENDER: std::sync::OnceLock<Sender<UiEvent>> = std::sync::OnceLock::new();

pub fn init_emitter(app: &AppHandle) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel::<UiEvent>();
    let app = app.clone();

    std::thread::Builder::new()
        .name("session-events".into())
        .spawn(move || {
            loop {
                let event = match rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(event) => event,
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                };

                let t0 = Instant::now();
                match event {
                    UiEvent::Status(status) => emit_status(&app, status),
                    UiEvent::Segments(segments) => emit_segments(&app, segments),
                    UiEvent::Level(level) => {
                        if let Err(e) = app.emit(EVENT_LEVEL, level) {
                            tracing::debug!("Не удалось передать уровень звука в окно: {e}");
                        }
                    }
                    UiEvent::Audio(status) => {
                        if let Err(e) = app.emit(EVENT_AUDIO, status) {
                            tracing::debug!("Не удалось передать состояние звука в окно: {e}");
                        }
                    }
                }

                let ms = t0.elapsed().as_millis();
                if ms >= 300 {
                    tracing::warn!(ms, "Передача события заняла слишком много времени");
                }
            }
            tracing::debug!("Поток событий завершён");
        })
        .map_err(|e| format!("Не удалось запустить поток событий: {e}"))?;

    let _ = EVENT_SENDER.set(tx);
    Ok(())
}

fn events_sender() -> Result<Sender<UiEvent>, String> {
    EVENT_SENDER
        .get()
        .cloned()
        .ok_or_else(|| "Поток событий не инициализирован".to_string())
}

enum TranslateCommand {
    Start {
        queue: Arc<JobQueue>,
        translator: Arc<Mutex<Box<dyn Translator>>>,
        store: Arc<Mutex<TranscriptStore>>,
        events: Sender<UiEvent>,
    },
}

static TRANSLATE_TX: std::sync::OnceLock<Sender<TranslateCommand>> = std::sync::OnceLock::new();

pub fn init_translate_worker() -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel::<TranslateCommand>();
    std::thread::Builder::new()
        .name("translate-worker".into())
        .spawn(move || {
            while let Ok(TranslateCommand::Start {
                queue,
                translator,
                store,
                events,
            }) = rx.recv()
            {
                while let Some(job) = queue.pop_blocking() {
                    let result = translator
                        .lock()
                        .expect("translator mutex poisoned")
                        .translate(&job.text)
                        .map_err(|e| e.to_string());

                    let updated = {
                        let mut s = store.lock().expect("transcript store mutex poisoned");
                        s.set_translation(&job.segment_id, result).ok()
                    };

                    if let Some(seg) = updated {
                        match seg.translation_state {
                            TranslationState::Done => {
                                tracing::info!(
                                    "[Перевод] {}",
                                    seg.translation.as_deref().unwrap_or("")
                                )
                            }
                            TranslationState::Failed => {
                                tracing::warn!(id = %seg.id, "[Перевод] Ошибка; для этого фрагмента отображается оригинал")
                            }
                            _ => {}
                        }
                        let _ = events.send(UiEvent::Segments(vec![seg]));
                    }
                }
                tracing::info!("Поток перевода ожидает задания");
            }
            tracing::debug!("Поток перевода завершён");
        })
        .map_err(|e| format!("Не удалось запустить поток перевода: {e}"))?;
    let _ = TRANSLATE_TX.set(tx);
    Ok(())
}

#[typeshare::typeshare]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub status: TaskStatus,
    pub device: Option<String>,
    pub device_sample_rate: Option<u32>,
}

#[derive(Default)]
pub struct SessionState {
    store: Arc<Mutex<TranscriptStore>>,
    running: Mutex<Option<RunningSession>>,
    status: Mutex<TaskStatus>,
    started_at: Mutex<Option<Instant>>,
    first_text_logged: AtomicBool,
    audio_started_at: Arc<Mutex<Option<Instant>>>,
    teardown_active: AtomicBool,
}

impl SessionState {
    fn note_first_text(&self) {
        if self.first_text_logged.swap(true, Ordering::SeqCst) {
            return;
        }
        let t0 = *self
            .started_at
            .lock()
            .expect("session start mutex poisoned");
        if let Some(t0) = t0 {
            let ms = t0.elapsed().as_millis();
            if ms <= FIRST_TEXT_BUDGET_MS {
                tracing::info!(
                    ms,
                    budget = FIRST_TEXT_BUDGET_MS,
                    "Первый текст получен в пределах допустимой задержки"
                );
            } else {
                tracing::warn!(
                    ms,
                    budget = FIRST_TEXT_BUDGET_MS,
                    "Первый текст получен с превышением допустимой задержки, включая загрузку модели при старте"
                );
            }
        }
    }
}

impl SessionState {
    pub fn status(&self) -> TaskStatus {
        *self.status.lock().expect("session status mutex poisoned")
    }

    fn set_status(&self, s: TaskStatus) {
        *self.status.lock().expect("session status mutex poisoned") = s;
    }

    pub fn snapshot(&self) -> Vec<TranscriptSegment> {
        self.store
            .lock()
            .expect("transcript store mutex poisoned")
            .snapshot()
            .to_vec()
    }

    pub fn is_running(&self) -> bool {
        self.running
            .lock()
            .expect("session running mutex poisoned")
            .is_some()
    }

    pub fn info(&self) -> SessionInfo {
        let guard = self.running.lock().expect("session running mutex poisoned");
        let status = self.status();
        match guard.as_ref() {
            Some(r) => SessionInfo {
                status,
                device: Some(r.device.clone()),
                device_sample_rate: Some(r.device_sample_rate),
            },
            None => SessionInfo {
                status,
                device: None,
                device_sample_rate: None,
            },
        }
    }
}

impl std::fmt::Debug for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionState")
            .field("status", &self.status())
            .field("running", &self.is_running())
            .field("segments", &self.snapshot().len())
            .finish()
    }
}

pub fn start(
    app: &AppHandle,
    cfg: SessionConfig,
    source: audio::AudioSourceKind,
    device: Option<String>,
) -> Result<(), String> {
    let mut cfg = cfg;
    cfg.translate.target_lang = target_lang_for(&cfg.translate.source_lang).to_owned();

    tracing::info!("Запуск: команда получена");
    let state = app.state::<SessionState>();

    if state.is_running() {
        return Err("Захват звука уже запущен".into());
    }

    let mut waited_ms = 0u64;
    while state.teardown_active.load(Ordering::SeqCst) && waited_ms < 5000 {
        std::thread::sleep(Duration::from_millis(100));
        waited_ms += 100;
    }
    if state.teardown_active.load(Ordering::SeqCst) {
        return Err("Предыдущий сеанс ещё завершается и удерживает аудиоустройство. Повторите попытку позже".into());
    }
    if waited_ms > 0 {
        tracing::info!(
            ms = waited_ms,
            "Ожидание завершения предыдущего сеанса перед запуском"
        );
    }

    let t0 = Instant::now();
    *state
        .started_at
        .lock()
        .expect("session start mutex poisoned") = Some(t0);
    state.first_text_logged.store(false, Ordering::SeqCst);
    *state
        .audio_started_at
        .lock()
        .expect("audio start mutex poisoned") = None;

    if !matches!(cfg.asr.engine_type, EngineType::LocalStreaming) {
        return Err(format!(
            "Движок {} не поддерживается; доступен local-streaming",
            cfg.asr.engine_type.as_str()
        ));
    }

    let prefs = crate::prefs::load(app);
    let asr_entry = catalog::asr_entry(prefs.asr_model_tier);

    let asr_dir = if let Some(custom) = cfg
        .asr
        .model_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let dir = dunce::simplified(std::path::Path::new(custom)).to_path_buf();
        if !dir.is_dir() {
            return Err(format!(
                "Каталог пользовательской модели ASR отсутствует: {}",
                dir.display()
            ));
        }
        crate::asr::streaming::discover_model_files(&dir).map_err(|e| e.to_string())?;
        tracing::info!(path = %dir.display(), "Используется каталог пользовательской модели ASR");
        dir
    } else {
        let roots = paths::search_roots(app);
        let resolved = paths::locate_asr(&roots, prefs.asr_model_tier)?;
        tracing::info!(
            tier = prefs.asr_model_tier.as_str(),
            via = resolved.via.as_str(),
            path = %resolved.path.display(),
            "Сеанс использует модель ASR"
        );
        let missing = paths::missing_asr_files(&resolved.path, asr_entry)?;
        if !missing.is_empty() {
            return Err(format!(
                "Модель распознавания «{}» не установлена или повреждена: {}\nОтсутствуют: {}\nВыберите встроенное распознавание на странице «Модели»",
                asr_entry.title,
                resolved.path.display(),
                missing.join(", ")
            ));
        }
        if !matches!(cfg.asr.source_lang.as_str(), "en" | "ru") {
            return Err(format!(
                "Неподдерживаемый язык распознавания: {}",
                cfg.asr.source_lang
            ));
        }
        resolved.path.join(&cfg.asr.source_lang)
    };

    let mut engine = crate::asr::streaming::StreamingAdapter::new();
    let engine_cfg = AsrConfig {
        model_dir: Some(asr_dir.to_string_lossy().into_owned()),
        ..cfg.asr.clone()
    };
    engine
        .init(&engine_cfg)
        .map_err(|e| format!("Не удалось инициализировать движок: {e}"))?;
    tracing::info!(
        caps = ?engine.capabilities(),
        ms = t0.elapsed().as_millis(),
        "Движок распознавания готов"
    );

    let engine: Arc<Mutex<Box<dyn AsrEngine>>> = Arc::new(Mutex::new(Box::new(engine)));

    let translator: Arc<Mutex<Box<dyn Translator>>> = Arc::new(Mutex::new(create_translator(
        app,
        &cfg.translate,
        prefs.mt_model_tier,
    )?));

    let queue = Arc::new(JobQueue::new(TRANSLATE_QUEUE_CAPACITY));
    let events = events_sender()?;

    let translate_tx = TRANSLATE_TX
        .get()
        .ok_or("Поток перевода не инициализирован")?
        .clone();
    translate_tx
        .send(TranslateCommand::Start {
            queue: Arc::clone(&queue),
            translator: Arc::clone(&translator),
            store: Arc::clone(&state.store),
            events: events.clone(),
        })
        .map_err(|_| "Поток перевода уже завершён".to_string())?;

    if let Err(e) = translator
        .lock()
        .expect("translator mutex poisoned")
        .init(&cfg.translate)
    {
        queue.close();
        return Err(format!("Не удалось инициализировать перевод: {e}"));
    }
    let caps = translator
        .lock()
        .expect("translator mutex poisoned")
        .capabilities();
    tracing::info!(
        engine = cfg.translate.engine.as_str(),
        ?caps,
        ms = t0.elapsed().as_millis(),
        "Движок перевода готов"
    );

    let store = Arc::clone(&state.store);
    let engine_for_sink = Arc::clone(&engine);
    let queue_for_sink = Arc::clone(&queue);
    let events_for_sink = events.clone();

    let mut last_emit = Instant::now()
        .checked_sub(EMIT_INTERVAL)
        .unwrap_or_else(Instant::now);
    let mut pending: Vec<TranscriptSegment> = Vec::new();

    let mut last_beat = Instant::now();
    let mut energy = 0.0f64;
    let mut energy_n = 0usize;
    let mut was_silent: Option<bool> = None;
    let mut last_level = Instant::now();
    let mut level_sq = 0.0f64;
    let mut level_n = 0usize;
    let mut level_emit_failed = false;

    tracing::info!("Запуск захвата звука");
    let audio_clock = Arc::clone(&state.audio_started_at);
    let capture = audio::start(source, device.as_deref(), move |chunk: &[f32]| {
        {
            let mut clock = audio_clock.lock().expect("audio start mutex poisoned");
            if clock.is_none() {
                *clock = Some(Instant::now());
            }
        }

        let sum_sq = chunk.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>();
        energy += sum_sq;
        energy_n += chunk.len();
        level_sq += sum_sq;
        level_n += chunk.len();

        if last_level.elapsed() >= LEVEL_INTERVAL {
            let level = audio::rms(level_sq, level_n);
            if events_for_sink.send(UiEvent::Level(level)).is_err() && !level_emit_failed {
                level_emit_failed = true;
                tracing::error!("Поток событий завершён; передача уровня звука остановлена");
            }
            last_level = Instant::now();
            level_sq = 0.0;
            level_n = 0;
        }

        if last_beat.elapsed() >= HEARTBEAT_INTERVAL {
            let rms = audio::rms(energy, energy_n);
            let count = store
                .lock()
                .expect("transcript store mutex poisoned")
                .segment_count();
            let silent = rms < SILENCE_RMS;

            if silent {
                let hint = if count > 0 {
                    "Текст уже распознавался. Возможно, сейчас пауза или тихий фрагмент"
                } else if matches!(
                    source,
                    audio::AudioSourceKind::Loopback | audio::AudioSourceKind::Mix
                ) {
                    "Текст ещё не распознан. Проверьте звук в проигрывателе и разрешение «Запись экрана и системного аудио»"
                } else {
                    "Текст ещё не распознан. Проверьте разрешение микрофона, доступность и выбор аудиоустройства"
                };

                if count > 0 {
                    if was_silent != Some(true) {
                        tracing::debug!(
                            rms = format!("{rms:.6}"),
                            segments = count,
                            source = source.as_str(),
                            "Уровень звука близок к нулю: {hint}"
                        );
                    }
                } else {
                    tracing::warn!(
                        rms = format!("{rms:.6}"),
                        segments = count,
                        source = source.as_str(),
                        "**Уровень звука близок к нулю**: {hint}"
                    );
                }
            } else {
                tracing::info!(
                    rms = format!("{rms:.4}"),
                    segments = count,
                    source = source.as_str(),
                    "Проверка активности"
                );
            }

            if was_silent != Some(silent) {
                was_silent = Some(silent);
                let _ = events_for_sink.send(UiEvent::Audio(AudioStatus {
                    silent,
                    rms,
                    segments: count as u32,
                }));
            }

            last_beat = Instant::now();
            energy = 0.0;
            energy_n = 0;
        }

        let deltas = {
            let mut eng = engine_for_sink.lock().expect("engine mutex poisoned");
            match eng.feed_pcm(chunk) {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!("Ошибка обработки звука: {e}");
                    return;
                }
            }
        };
        if deltas.is_empty() {
            return;
        }

        let to_translate = {
            let mut s = store.lock().expect("transcript store mutex poisoned");
            match s.apply(deltas) {
                Ok(changed) => {
                    for seg in &changed {
                        if seg.state != crate::transcript::SegmentState::Partial {
                            let audio_ms: i128 = audio_clock
                                .lock()
                                .expect("audio start mutex poisoned")
                                .map(|a| a.elapsed().as_millis() as i128)
                                .unwrap_or(0);
                            tracing::info!(
                                start_ms = seg.start_ms,
                                end_ms = seg.end_ms,
                                gap_ms = audio_ms - seg.end_ms as i128,
                                "[Готовый фрагмент] {}",
                                seg.text
                            );
                        }
                    }

                    let ids: Vec<String> = changed
                        .iter()
                        .filter(|s| {
                            s.state.is_frozen() && s.translation_state == TranslationState::Idle
                        })
                        .map(|s| s.id.clone())
                        .collect();

                    let marked = s.mark_pending_translation(&ids);
                    pending.extend(changed);
                    marked
                }
                Err(e) => {
                    tracing::error!("Результат движка отклонён при обновлении фрагмента: {e}");
                    return;
                }
            }
        };

        for seg in to_translate {
            let dropped = queue_for_sink.push(Job {
                segment_id: seg.id.clone(),
                text: seg.text.clone(),
            });
            if let Some(d) = dropped {
                tracing::warn!(
                    dropped_ms = d.segment_id,
                    "Очередь перевода переполнена; удалено самое старое задание"
                );
            }
            pending.push(seg);
        }

        if last_emit.elapsed() >= EMIT_INTERVAL && !pending.is_empty() {
            let batch = coalesce(std::mem::take(&mut pending));
            let _ = events_for_sink.send(UiEvent::Segments(batch));
            last_emit = Instant::now();
        }
    })
    .map_err(|e| format!("Не удалось запустить захват звука: {e}"))?;

    let device_name = capture.device_name().to_string();
    let device_sample_rate = capture.device_sample_rate();

    tracing::info!(
        device = %device_name,
        device_sample_rate,
        target_sample_rate = TARGET_SAMPLE_RATE,
        setup_ms = t0.elapsed().as_millis(),
        "Сеанс распознавания запущен"
    );

    let _ = events.send(UiEvent::Status(TaskStatus::Capturing));

    *state
        .running
        .lock()
        .expect("session running mutex poisoned") = Some(RunningSession {
        capture,
        engine,
        device: device_name,
        device_sample_rate,
        queue,
        translator,
        events,
    });
    state.set_status(TaskStatus::Capturing);

    crate::telemetry::track(
        app,
        "session_started",
        Some(serde_json::json!({
            "audio_source": source.as_str(),
            "translate_engine": cfg.translate.engine.as_str(),
            "source_lang": cfg.translate.source_lang,
            "asr_model_tier": prefs.asr_model_tier.as_str(),
            "custom_asr_model": if cfg.asr.model_dir.as_deref().is_some_and(|s| !s.trim().is_empty()) { 1 } else { 0 },
        })),
    );

    Ok(())
}

pub fn stop(app: &AppHandle) -> Result<(), String> {
    tracing::info!("Остановка: команда получена");
    let state = app.state::<SessionState>();

    let Some(session) = state
        .running
        .lock()
        .expect("session running mutex poisoned")
        .take()
    else {
        state.set_status(TaskStatus::Ready);
        tracing::info!("Остановка: активный сеанс отсутствует");
        return Ok(());
    };
    tracing::info!("Остановка: активный сеанс отсоединён");

    let audio_ms = {
        let clock = state
            .audio_started_at
            .lock()
            .expect("audio start mutex poisoned");
        (*clock)
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0)
    };
    crate::telemetry::track(
        app,
        "session_stopped",
        Some(serde_json::json!({
            "duration_bucket": crate::telemetry::duration_bucket(audio_ms),
            "segments": state.snapshot().len() as u64,
        })),
    );

    session.capture.request_stop();
    session.queue.close();

    state.set_status(TaskStatus::Ready);
    let _ = session.events.send(UiEvent::Status(TaskStatus::Ready));
    tracing::info!("Остановка: состояние готовности восстановлено; завершение продолжается в фоне");

    state.teardown_active.store(true, Ordering::SeqCst);

    let app_for_teardown = app.clone();
    let task: TeardownTask = Box::new(move || finish_teardown(&app_for_teardown, session));

    match TEARDOWN_TX.get() {
        Some(tx) => {
            if let Err(err) = tx.send(task) {
                tracing::error!("Поток завершения закрыт; сеанс завершается в потоке команд");
                (err.0)();
            }
        }
        None => {
            tracing::error!(
                "Поток завершения не инициализирован; сеанс завершается в потоке команд"
            );
            task();
        }
    }

    Ok(())
}

type TeardownTask = Box<dyn FnOnce() + Send>;

static TEARDOWN_TX: std::sync::OnceLock<std::sync::mpsc::Sender<TeardownTask>> =
    std::sync::OnceLock::new();

pub fn init_teardown_executor() -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel::<TeardownTask>();
    std::thread::Builder::new()
        .name("session-teardown".into())
        .spawn(move || {
            while let Ok(task) = rx.recv() {
                task();
            }
            tracing::debug!("Поток завершения закрыт");
        })
        .map_err(|e| format!("Не удалось запустить поток завершения: {e}"))?;
    let _ = TEARDOWN_TX.set(tx);
    Ok(())
}

fn finish_teardown(app: &AppHandle, session: RunningSession) {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        teardown(app, session);
    }));
    if outcome.is_err() {
        tracing::error!("Поток завершения аварийно закрыт");
    }
    app.state::<SessionState>()
        .teardown_active
        .store(false, Ordering::SeqCst);
}

fn teardown(app: &AppHandle, session: RunningSession) {
    let RunningSession {
        capture,
        engine,
        queue,
        translator,
        events,
        ..
    } = session;
    let t0 = Instant::now();

    tracing::info!("Завершение: ожидание закрытия потока захвата");
    drop(capture);
    tracing::info!(
        ms = t0.elapsed().as_millis(),
        "Завершение: поток захвата закрыт"
    );

    tracing::info!("Завершение: получение оставшегося текста");
    let tail = {
        let mut eng = engine.lock().expect("engine mutex poisoned");
        let flushed = eng.flush();
        eng.shutdown();
        flushed
    };
    let changed = match tail {
        Ok(tail) if !tail.is_empty() => {
            let state = app.state::<SessionState>();
            let applied = {
                let mut s = state.store.lock().expect("transcript store mutex poisoned");
                s.apply(tail)
            };
            match applied {
                Ok(changed) => changed,
                Err(e) => {
                    tracing::error!("Последний фрагмент отклонён: {e}");
                    Vec::new()
                }
            }
        }
        Ok(_) => Vec::new(),
        Err(e) => {
            tracing::error!("Не удалось получить оставшийся текст: {e}");
            Vec::new()
        }
    };
    tracing::info!(ms = t0.elapsed().as_millis(), "Завершение: движок закрыт");

    let dropped = queue.clear();
    if dropped > 0 {
        tracing::info!(
            dropped,
            "При остановке удалены накопившиеся задания перевода"
        );
    }
    queue.close();

    tracing::info!(
        ms = t0.elapsed().as_millis(),
        "Завершение: очередь перевода закрыта"
    );

    tracing::info!("Завершение: освобождение движка перевода");
    translator
        .lock()
        .expect("translator mutex poisoned")
        .shutdown();
    tracing::info!(ms = t0.elapsed().as_millis(), "Сеанс полностью завершён");

    if !changed.is_empty() {
        let _ = events.send(UiEvent::Segments(changed));
    }
}

pub fn clear(app: &AppHandle) {
    let state = app.state::<SessionState>();
    {
        let mut store = state.store.lock().expect("transcript store mutex poisoned");
        *store = TranscriptStore::new();
    }
    emit_segments(app, Vec::new());
}

fn coalesce(batch: Vec<TranscriptSegment>) -> Vec<TranscriptSegment> {
    let mut latest: HashMap<String, TranscriptSegment> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for seg in batch {
        if !latest.contains_key(&seg.id) {
            order.push(seg.id.clone());
        }
        let keep = latest
            .get(&seg.id)
            .is_none_or(|e| seg.revision >= e.revision);
        if keep {
            latest.insert(seg.id.clone(), seg);
        }
    }

    order
        .into_iter()
        .filter_map(|id| latest.remove(&id))
        .collect()
}

fn emit_segments(app: &AppHandle, segments: Vec<TranscriptSegment>) {
    if segments.iter().any(|s| !s.text.trim().is_empty()) {
        app.state::<SessionState>().note_first_text();
    }

    if let Err(e) = app.emit(EVENT_SEGMENTS, segments) {
        tracing::debug!("Не удалось передать фрагмент в окно: {e}");
    }
}

fn emit_status(app: &AppHandle, status: TaskStatus) {
    crate::tray::sync_task_state(app, status);
    if let Err(e) = app.emit(EVENT_TASK_STATE, status) {
        tracing::debug!("Не удалось передать состояние задания в окно: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::SegmentState;

    fn seg(id: &str, text: &str, rev: u32) -> TranscriptSegment {
        TranscriptSegment {
            id: id.into(),
            start_ms: 0,
            end_ms: 100,
            text: text.into(),
            translation: None,
            translation_state: crate::transcript::TranslationState::Idle,
            state: SegmentState::Partial,
            revision: rev,
        }
    }

    #[test]
    fn coalesce_keeps_latest_revision_per_id() {
        let batch = vec![
            seg("a", "今", 1),
            seg("a", "今天", 2),
            seg("b", "另", 1),
            seg("a", "今天下午", 3),
        ];
        let out = coalesce(batch);

        assert_eq!(out.len(), 2, "应按 id 去重");
        assert_eq!(out[0].id, "a");
        assert_eq!(out[0].text, "今天下午", "应保留最新 revision");
        assert_eq!(out[1].id, "b", "顺序应按首次出现");
    }

    #[test]
    fn coalesce_does_not_let_an_older_revision_win() {
        let out = coalesce(vec![seg("a", "新", 5), seg("a", "旧", 2)]);
        assert_eq!(out[0].text, "新");
    }
}
