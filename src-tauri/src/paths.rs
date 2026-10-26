use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use crate::config::{AsrModelTier, MtModelTier};
use crate::models::catalog::{self, CatalogEntry};

pub const ENV_MODEL_DIR: &str = "AUDIOSCRIPT_MODEL_DIR";

#[derive(Debug, Clone)]
pub struct ResolvedModelDir {
    pub path: PathBuf,
    pub via: ResolutionVia,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionVia {
    EnvOverride,
    BundledResource,
    DevSourceTree,
    UserData,
}

impl ResolutionVia {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EnvOverride => "env-override",
            Self::BundledResource => "bundled-resource",
            Self::DevSourceTree => "dev-source-tree",
            Self::UserData => "user-data",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModelSearchRoots {
    pub env: Option<PathBuf>,
    pub bundled: Option<PathBuf>,
    pub dev: Option<PathBuf>,
    pub user: Option<PathBuf>,
}

fn simplify(path: PathBuf) -> PathBuf {
    dunce::simplified(&path).to_path_buf()
}

fn resolved(path: PathBuf, via: ResolutionVia) -> ResolvedModelDir {
    ResolvedModelDir {
        path: simplify(path),
        via,
    }
}

pub fn user_models_root(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_local_data_dir()
        .ok()
        .map(|dir| simplify(dir.join("models")))
}

pub fn ensure_user_models_root(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = user_models_root(app).ok_or_else(|| {
        "Не удалось определить каталог данных приложения для сохранения моделей".to_string()
    })?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Не удалось создать {}: {e}", dir.display()))?;
    Ok(dir)
}

pub fn search_roots(app: &AppHandle) -> ModelSearchRoots {
    let env = std::env::var(ENV_MODEL_DIR)
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_dir());

    let bundled = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| simplify(dir.join("models")))
        .filter(|p| p.is_dir());

    let dev = {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/models");
        p.is_dir().then_some(p)
    };

    ModelSearchRoots {
        env,
        bundled,
        dev,
        user: user_models_root(app),
    }
}

fn add_candidate(
    candidates: &mut Vec<(PathBuf, ResolutionVia)>,
    root: &Option<PathBuf>,
    rel: &str,
    via: ResolutionVia,
) {
    if let Some(root) = root {
        candidates.push((root.join(rel), via));
    }
}

fn first_existing(
    candidates: Vec<(PathBuf, ResolutionVia)>,
    what: &str,
    remedy: &str,
) -> Result<ResolvedModelDir, String> {
    let mut tried = Vec::new();
    for (path, via) in candidates {
        if path.is_dir() {
            return Ok(resolved(path, via));
        }
        tried.push(format!("{} (отсутствует)", path.display()));
    }

    if tried.is_empty() {
        tried.push("Нет доступных каталогов моделей".to_string());
    }

    Err(format!(
        "Не найдено: {what}. Проверены каталоги:\n  - {}\n\n{remedy}",
        tried.join("\n  - ")
    ))
}

fn locate(
    roots: &ModelSearchRoots,
    bundled: bool,
    rel: &str,
    what: String,
    remedy: String,
) -> Result<ResolvedModelDir, String> {
    let mut candidates = Vec::new();
    add_candidate(&mut candidates, &roots.env, rel, ResolutionVia::EnvOverride);
    if bundled {
        add_candidate(
            &mut candidates,
            &roots.bundled,
            rel,
            ResolutionVia::BundledResource,
        );
        add_candidate(
            &mut candidates,
            &roots.dev,
            rel,
            ResolutionVia::DevSourceTree,
        );
    } else {
        add_candidate(&mut candidates, &roots.user, rel, ResolutionVia::UserData);
    }
    first_existing(candidates, &what, &remedy)
}

pub fn locate_asr(
    roots: &ModelSearchRoots,
    tier: AsrModelTier,
) -> Result<ResolvedModelDir, String> {
    let entry = catalog::asr_entry(tier);
    let bundled = matches!(tier, AsrModelTier::Builtin);
    let remedy = if bundled {
        format!(
            "Встроенные модели отсутствуют. Переустановите приложение; для разработки выполните `bun run fetch:model` и перезапустите.\n\
             (Каталог {ENV_MODEL_DIR}, если задан, проверяется первым.)"
        )
    } else {
        format!(
            "Выберите встроенное распознавание на странице «Модели».\n\
             (Каталог {ENV_MODEL_DIR}, если задан, проверяется первым.)"
        )
    };
    locate(
        roots,
        bundled,
        entry.installed_dir(),
        format!("Модель распознавания «{}»", entry.title),
        remedy,
    )
}

pub fn locate_mt_root(
    roots: &ModelSearchRoots,
    tier: MtModelTier,
) -> Result<ResolvedModelDir, String> {
    let entry = catalog::mt_entry(tier);
    let bundled = matches!(tier, MtModelTier::Base);
    let remedy = if bundled {
        format!(
            "Встроенные модели отсутствуют. Переустановите приложение; для разработки выполните `bun run fetch:mt` и перезапустите.\n\
             (Каталог {ENV_MODEL_DIR}, если задан, проверяется первым.)"
        )
    } else {
        format!(
            "Выберите встроенный перевод на странице «Модели».\n\
             (Каталог {ENV_MODEL_DIR}, если задан, проверяется первым.)"
        )
    };
    locate(
        roots,
        bundled,
        entry.installed_dir(),
        format!("Модель перевода «{}»", entry.title),
        remedy,
    )
}

fn file_names(dir: &Path) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Не удалось прочитать {}: {e}", dir.display()))?;
    Ok(entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect())
}

fn matches_precision(name: &str, prefix: &str, precision: &str) -> bool {
    if !(name.starts_with(prefix) && name.ends_with(".onnx")) {
        return false;
    }
    match precision {
        "int8" => name.contains(".int8."),
        _ => !name.contains(".int8."),
    }
}

pub fn missing_asr_files(dir: &Path, entry: &CatalogEntry) -> Result<Vec<String>, String> {
    let req = entry
        .required
        .asr
        .as_ref()
        .ok_or_else(|| format!("В каталоге нет требований ASR для {}", entry.id))?;
    if !req.languages.is_empty() {
        let mut missing = Vec::new();
        for language in &req.languages {
            let child = dir.join(language);
            if !child.is_dir() {
                missing.push(format!("{language}/ (каталог отсутствует)"));
                continue;
            }
            missing.extend(
                missing_asr_language_files(&child, req)?
                    .into_iter()
                    .map(|f| format!("{language}/{f}")),
            );
        }
        return Ok(missing);
    }
    missing_asr_language_files(dir, req)
}

fn missing_asr_language_files(
    dir: &Path,
    req: &catalog::AsrRequired,
) -> Result<Vec<String>, String> {
    let names = file_names(dir)?;
    let mut missing = Vec::new();
    for (prefix, precision) in [
        ("encoder", req.encoder.as_str()),
        ("decoder", req.decoder.as_str()),
        ("joiner", req.joiner.as_str()),
    ] {
        if !names
            .iter()
            .any(|n| matches_precision(n, prefix, precision))
        {
            missing.push(format!("{prefix}*.onnx（{precision}）"));
        }
    }
    for f in &req.files {
        if !names.iter().any(|n| n == f) {
            missing.push(f.clone());
        }
    }

    Ok(missing)
}

pub fn missing_mt_files(root: &Path, entry: &CatalogEntry) -> Result<Vec<String>, String> {
    let req = entry
        .required
        .mt
        .as_ref()
        .ok_or_else(|| format!("В каталоге отсутствуют требования MT для {}", entry.id))?;

    let directions = entry.direction_dirs();
    if directions.is_empty() {
        return Err(format!(
            "В каталоге для {} не заданы направления перевода",
            entry.id
        ));
    }

    let mut missing = Vec::new();
    for dir in directions {
        for f in &req.files {
            if !root.join(dir).join(f).is_file() {
                missing.push(format!("{dir}/{f}"));
            }
        }
    }

    Ok(missing)
}

pub fn model_dir_is_complete(dir: &Path) -> Result<(), String> {
    let entry = catalog::asr_entry(AsrModelTier::Builtin);
    let missing = missing_asr_language_files(dir, entry.required.asr.as_ref().unwrap())?;
    if missing.is_empty() {
        return Ok(());
    }

    let names = file_names(dir).unwrap_or_default();
    Err(format!(
        "В каталоге модели {} отсутствуют: {}\nСодержимое: {}",
        dir.display(),
        missing.join(", "),
        names.join(", ")
    ))
}

pub fn log_startup_selfcheck(app: &AppHandle) {
    let prefs = crate::prefs::load(app);
    let roots = search_roots(app);

    match locate_asr(&roots, prefs.asr_model_tier) {
        Ok(resolved) => {
            tracing::info!(
                tier = prefs.asr_model_tier.as_str(),
                via = resolved.via.as_str(),
                path = %resolved.path.display(),
                "Определён каталог модели ASR"
            );
            match missing_asr_files(
                &resolved.path,
                catalog::asr_entry(prefs.asr_model_tier),
            ) {
                Ok(missing) if missing.is_empty() => {}
                Ok(missing) => tracing::error!(
                    "Отсутствуют файлы ASR: {}\nВыберите встроенное распознавание на странице «Модели»",
                    missing.join(", ")
                ),
                Err(e) => tracing::error!("{e}"),
            }
        }
        Err(e) => tracing::error!("{e}"),
    }

    match locate_mt_root(&roots, prefs.mt_model_tier) {
        Ok(resolved) => {
            tracing::info!(
                tier = prefs.mt_model_tier.as_str(),
                via = resolved.via.as_str(),
                path = %resolved.path.display(),
                "Определён каталог модели перевода"
            );
            match missing_mt_files(&resolved.path, catalog::mt_entry(prefs.mt_model_tier)) {
                Ok(missing) if missing.is_empty() => {}
                Ok(missing) => tracing::error!(
                    "Отсутствуют файлы перевода: {}\nВыберите встроенный перевод на странице «Модели»",
                    missing.join(", ")
                ),
                Err(e) => tracing::error!("{e}"),
            }
        }
        Err(e) => tracing::error!("{e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("paths-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        for f in files {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        dir
    }

    fn root_with(dir_name: &str, files: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("paths-root-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join(dir_name)).unwrap();
        for f in files {
            std::fs::write(root.join(dir_name).join(f), b"x").unwrap();
        }
        root
    }

    #[test]
    fn complete_when_all_four_pieces_present() {
        let dir = temp_dir(&[
            "encoder-epoch-99-avg-1.int8.onnx",
            "decoder-epoch-99-avg-1.onnx",
            "joiner-epoch-99-avg-1.int8.onnx",
            "tokens.txt",
        ]);
        assert!(model_dir_is_complete(&dir).is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn incomplete_error_names_what_is_missing() {
        let dir = temp_dir(&["encoder-epoch-99-avg-1.int8.onnx", "tokens.txt"]);
        let msg = model_dir_is_complete(&dir).unwrap_err();
        assert!(msg.contains("decoder"), "{msg}");
        assert!(msg.contains("joiner"), "{msg}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn subdirectories_do_not_count_as_files() {
        let dir = temp_dir(&["tokens.txt"]);
        std::fs::create_dir_all(dir.join("encoder-x.onnx")).unwrap();
        let msg = model_dir_is_complete(&dir).unwrap_err();
        assert!(msg.contains("encoder"), "{msg}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_dir_is_an_error_not_a_panic() {
        assert!(model_dir_is_complete(Path::new("/nonexistent/models")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn verbatim_resource_paths_are_simplified() {
        let r = resolved(
            PathBuf::from(r"\\?\C:\Users\x\AppData\Local\AudioScript\models"),
            ResolutionVia::BundledResource,
        );
        assert_eq!(
            r.path,
            Path::new(r"C:\Users\x\AppData\Local\AudioScript\models")
        );
    }

    #[test]
    fn builtin_prefers_env_over_bundled_and_dev() {
        let env = root_with(
            catalog::asr_entry(AsrModelTier::Builtin).installed_dir(),
            &[],
        );
        let bundled = root_with(
            catalog::asr_entry(AsrModelTier::Builtin).installed_dir(),
            &[],
        );
        let dev = root_with(
            catalog::asr_entry(AsrModelTier::Builtin).installed_dir(),
            &[],
        );
        let roots = ModelSearchRoots {
            env: Some(env.clone()),
            bundled: Some(bundled),
            dev: Some(dev),
            user: None,
        };
        let got = locate_asr(&roots, AsrModelTier::Builtin).unwrap();
        assert_eq!(got.via, ResolutionVia::EnvOverride);
        assert!(got.path.starts_with(&env));
        std::fs::remove_dir_all(&env).ok();
    }

    #[test]
    fn optional_tier_uses_user_data_and_never_bundled() {
        let rel = catalog::asr_entry(AsrModelTier::HighAccuracy).installed_dir();
        let user = root_with(rel, &[]);
        let bundled = root_with(rel, &[]);
        let roots = ModelSearchRoots {
            env: None,
            bundled: Some(bundled.clone()),
            dev: None,
            user: Some(user.clone()),
        };
        let got = locate_asr(&roots, AsrModelTier::HighAccuracy).unwrap();
        assert_eq!(got.via, ResolutionVia::UserData);
        assert!(got.path.starts_with(&user));

        let bundled_only = ModelSearchRoots {
            env: None,
            bundled: Some(bundled),
            dev: None,
            user: None,
        };
        assert!(locate_asr(&bundled_only, AsrModelTier::HighAccuracy).is_err());
        std::fs::remove_dir_all(&user).ok();
    }

    #[test]
    fn first_existing_incomplete_dir_wins() {
        let rel = catalog::asr_entry(AsrModelTier::HighAccuracy).installed_dir();
        let user = root_with(rel, &[]);
        let roots = ModelSearchRoots {
            env: None,
            bundled: None,
            dev: None,
            user: Some(user.clone()),
        };
        let got = locate_asr(&roots, AsrModelTier::HighAccuracy).unwrap();
        assert!(got.path.starts_with(&user));
        let missing =
            missing_asr_files(&got.path, catalog::asr_entry(AsrModelTier::HighAccuracy)).unwrap();
        assert!(missing.iter().any(|m| m.starts_with("en/")), "{missing:?}");
        std::fs::remove_dir_all(&user).ok();
    }

    #[test]
    fn asr_precision_is_enforced() {
        let dir = temp_dir(&[
            "encoder-x.onnx",
            "decoder-x.onnx",
            "joiner-x.int8.onnx",
            "tokens.txt",
        ]);
        let missing = missing_asr_language_files(
            &dir,
            catalog::asr_entry(AsrModelTier::Builtin)
                .required
                .asr
                .as_ref()
                .unwrap(),
        )
        .unwrap();
        assert!(missing.iter().any(|m| m.contains("encoder")), "{missing:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mt_missing_files_name_direction_and_file() {
        let entry = catalog::mt_entry(MtModelTier::Base);
        let root = std::env::temp_dir().join(format!("paths-mt-{}", uuid::Uuid::new_v4()));
        let missing = missing_mt_files(&root, entry).unwrap();
        assert!(
            missing.iter().any(|m| m.contains("model.bin")),
            "{missing:?}"
        );
        assert!(
            missing.iter().all(|m| m.contains('/')),
            "必须带上方向目录：{missing:?}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn mt_base_locates_bundled_mt_dir() {
        let rel = catalog::mt_entry(MtModelTier::Base).installed_dir();
        let bundled = root_with(rel, &[]);
        let roots = ModelSearchRoots {
            env: None,
            bundled: Some(bundled.clone()),
            dev: None,
            user: None,
        };
        let got = locate_mt_root(&roots, MtModelTier::Base).unwrap();
        assert_eq!(got.via, ResolutionVia::BundledResource);
        assert!(got.path.ends_with(rel));
        std::fs::remove_dir_all(&bundled).ok();
    }
}
