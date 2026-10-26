use std::path::{Path, PathBuf};

use ct2rs::tokenizers::hf::Tokenizer as HfTokenizer;

use super::{TranslateCaps, TranslateError, Translator};
use crate::config::{MtModelTier, TranslateConfig};
use crate::models::catalog;

const SOURCE_TOKENIZER: &str = "source-tokenizer.json";
const TARGET_TOKENIZER: &str = "target-tokenizer.json";

pub struct MarianTokenizer {
    source: HfTokenizer,
    target: HfTokenizer,
}

impl MarianTokenizer {
    fn load(dir: &Path) -> Result<Self, TranslateError> {
        let load = |f: &str| {
            HfTokenizer::from_file(dir.join(f))
                .map_err(|e| TranslateError::ModelLoad(format!("{}/{f}：{e}", dir.display())))
        };
        Ok(Self {
            source: load(SOURCE_TOKENIZER)?,
            target: load(TARGET_TOKENIZER)?,
        })
    }
}

impl ct2rs::Tokenizer for MarianTokenizer {
    fn encode(&self, input: &str) -> anyhow::Result<Vec<String>> {
        let mut tokens = ct2rs::Tokenizer::encode(&self.source, input)?;

        tokens.push("</s>".to_string());
        Ok(tokens)
    }

    fn decode(&self, tokens: Vec<String>) -> anyhow::Result<String> {
        ct2rs::Tokenizer::decode(&self.target, tokens)
    }
}

pub struct LocalNmt {
    engine: Option<ct2rs::Translator<MarianTokenizer>>,
    direction: Direction,
    tier: MtModelTier,
    model_root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    EnRu,
    RuEn,
}

impl Direction {
    fn dir_name(self, tier: MtModelTier) -> &'static str {
        let dirs = catalog::mt_entry(tier).direction_dirs();
        match self {
            Self::EnRu => dirs
                .first()
                .map(String::as_str)
                .expect("В каталоге отсутствует направление en→ru"),
            Self::RuEn => dirs
                .get(1)
                .map(String::as_str)
                .expect("В каталоге отсутствует направление ru→en"),
        }
    }

    fn resolve(from: &str, to: &str) -> Result<Self, TranslateError> {
        let from = from.to_ascii_lowercase();
        let to = to.to_ascii_lowercase();

        if from == "auto" {
            return Err(TranslateError::UnsupportedPair {
                from: "auto (выберите исходный язык явно)".into(),
                to: to.to_string(),
            });
        }
        let from_is_en = from.split('-').next() == Some("en");
        let from_is_ru = from.split('-').next() == Some("ru");
        let to_is_ru = to.split('-').next() == Some("ru");
        let to_is_en = to.split('-').next() == Some("en");

        match (from_is_en || from_is_ru, to_is_ru, to_is_en) {
            (true, true, _) if !from_is_ru => Ok(Self::EnRu),
            (true, _, true) if !from_is_en => Ok(Self::RuEn),
            _ => Err(TranslateError::UnsupportedPair {
                from: from.to_string(),
                to: to.to_string(),
            }),
        }
    }
}

impl LocalNmt {
    pub fn new(model_root: PathBuf, tier: MtModelTier) -> Self {
        Self {
            engine: None,
            direction: Direction::EnRu,
            tier,
            model_root,
        }
    }

    fn model_dir(&self) -> PathBuf {
        self.model_root.join(self.direction.dir_name(self.tier))
    }

    fn check_model_dir(dir: &Path, tier: MtModelTier) -> Result<(), TranslateError> {
        let remedy = match tier {
            MtModelTier::Base => {
                "Запустите `bun run fetch:mt`, чтобы загрузить встроенные модели перевода"
            }
            MtModelTier::Big => "Выберите встроенный перевод на странице «Модели»",
        };

        if !dir.is_dir() {
            return Err(TranslateError::MissingModelFile(format!(
                "{} отсутствует.\n{remedy}",
                dir.display()
            )));
        }

        for f in ["model.bin", SOURCE_TOKENIZER, TARGET_TOKENIZER] {
            if !dir.join(f).is_file() {
                return Err(TranslateError::MissingModelFile(format!(
                    "{}/{f} отсутствует.\n{remedy}",
                    dir.display()
                )));
            }
        }
        Ok(())
    }
}

impl Translator for LocalNmt {
    fn init(&mut self, cfg: &TranslateConfig) -> Result<(), TranslateError> {
        self.direction = Direction::resolve(&cfg.source_lang, &cfg.target_lang)?;

        if let Some(dir) = &cfg.model_dir {
            self.model_root = dunce::simplified(Path::new(dir)).to_path_buf();
        }

        let dir = self.model_dir();
        Self::check_model_dir(&dir, self.tier)?;

        let config = ct2rs::Config {
            num_threads_per_replica: 1,
            ..Default::default()
        };

        let tok = MarianTokenizer::load(&dir)?;
        let engine = ct2rs::Translator::with_tokenizer(&dir, tok, &config)
            .map_err(|e| TranslateError::ModelLoad(format!("{}：{e}", dir.display())))?;

        self.engine = Some(engine);

        let warm = match self.direction {
            Direction::EnRu => "hello",
            Direction::RuEn => "Привет",
        };
        if let Err(e) = self.translate(warm) {
            tracing::warn!("Ошибка при прогреве движка перевода: {e}");
        }

        Ok(())
    }

    fn capabilities(&self) -> TranslateCaps {
        TranslateCaps::LOCAL_NMT
    }

    fn translate(&mut self, text: &str) -> Result<String, TranslateError> {
        let engine = self.engine.as_ref().ok_or(TranslateError::NotInitialized)?;

        let sources = vec![text.to_string()];
        let options = ct2rs::TranslationOptions {
            beam_size: 1,
            ..Default::default()
        };

        let results = engine
            .translate_batch(&sources, &options, None)
            .map_err(|e| TranslateError::Internal(e.to_string()))?;

        let (translated, _) = results
            .into_iter()
            .next()
            .ok_or_else(|| TranslateError::Internal("Движок вернул пустой результат".into()))?;

        if let Some(reason) = crate::translate::degenerate_translation(&translated) {
            return Err(TranslateError::Degenerate { reason });
        }

        Ok(translated)
    }

    fn shutdown(&mut self) {
        self.engine = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_resolves_both_ways() {
        assert_eq!(Direction::resolve("en", "ru").unwrap(), Direction::EnRu);
        assert_eq!(Direction::resolve("ru", "en").unwrap(), Direction::RuEn);
        assert!(matches!(
            Direction::resolve("auto", "ru"),
            Err(TranslateError::UnsupportedPair { .. })
        ));
        assert_eq!(
            Direction::resolve("en-US", "ru-RU").unwrap(),
            Direction::EnRu
        );
    }

    #[test]
    fn unsupported_pairs_are_rejected_not_silently_defaulted() {
        assert!(matches!(
            Direction::resolve("ja", "ru"),
            Err(TranslateError::UnsupportedPair { .. })
        ));
        assert!(matches!(
            Direction::resolve("en", "fr"),
            Err(TranslateError::UnsupportedPair { .. })
        ));
        assert!(Direction::resolve("ru", "ru").is_err());
    }

    #[test]
    fn missing_model_dir_gives_actionable_error() {
        let e =
            LocalNmt::check_model_dir(Path::new("/nonexistent/mt"), MtModelTier::Base).unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("fetch:mt"), "错误里要给出修复动作：{msg}");

        let e =
            LocalNmt::check_model_dir(Path::new("/nonexistent/mt"), MtModelTier::Big).unwrap_err();
        assert!(e.to_string().contains("Модели"), "{e}");
    }

    #[test]
    fn tiers_map_to_their_direction_directories() {
        assert_eq!(
            Direction::EnRu.dir_name(MtModelTier::Base),
            "opus-mt-en-ru-ct2"
        );
        assert_eq!(
            Direction::RuEn.dir_name(MtModelTier::Base),
            "opus-mt-ru-en-ct2"
        );
        assert_eq!(
            Direction::EnRu.dir_name(MtModelTier::Big),
            "opus-mt-tc-big-en-ru-ct2"
        );
        assert_eq!(
            Direction::RuEn.dir_name(MtModelTier::Big),
            "opus-mt-tc-big-ru-en-ct2"
        );
    }

    #[test]
    fn translate_before_init_is_rejected() {
        let mut t = LocalNmt::new(PathBuf::from("/tmp"), MtModelTier::Base);
        assert!(matches!(
            t.translate("hello"),
            Err(TranslateError::NotInitialized)
        ));
    }
    #[test]
    #[ignore = "Requires downloaded bundled translation models"]
    fn bundled_translation_canary() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/models/mt");
        let mut translator = LocalNmt::new(root, MtModelTier::Base);
        for (source, target, text) in [
            ("en", "ru", "The cat is sleeping."),
            ("ru", "en", "Кошка спит."),
        ] {
            translator
                .init(&TranslateConfig {
                    source_lang: source.into(),
                    target_lang: target.into(),
                    ..Default::default()
                })
                .unwrap();
            let output = translator.translate(text).unwrap();
            println!("{source} → {target}: {text} => {output}");
            let lower = output.to_lowercase();
            if target == "ru" {
                assert!(
                    lower.contains("спит") && (lower.contains("кот") || lower.contains("кошк")),
                    "{output}"
                );
            } else {
                assert!(lower.contains("cat") && lower.contains("sleep"), "{output}");
            }
            translator.shutdown();
        }
    }
}
