use std::time::Duration;

use super::{TranslateCaps, TranslateError, Translator};
use crate::config::TranslateConfig;

pub const DEFAULT_DELAY: Duration = Duration::from_millis(400);

pub const ENV_FAIL_EVERY: &str = "AUDIOSCRIPT_TRANSLATE_FAIL_EVERY";

pub struct MockTranslator {
    initialized: bool,
    delay: Duration,
    fail_every: u32,
    seen: u32,
}

impl Default for MockTranslator {
    fn default() -> Self {
        Self::new(DEFAULT_DELAY, 0)
    }
}

impl MockTranslator {
    pub fn new(delay: Duration, fail_every: u32) -> Self {
        Self {
            initialized: false,
            delay,
            fail_every,
            seen: 0,
        }
    }
}

impl Translator for MockTranslator {
    fn init(&mut self, _cfg: &TranslateConfig) -> Result<(), TranslateError> {
        self.initialized = true;
        Ok(())
    }

    fn capabilities(&self) -> TranslateCaps {
        TranslateCaps::LOCAL_NMT
    }

    fn translate(&mut self, text: &str) -> Result<String, TranslateError> {
        if !self.initialized {
            return Err(TranslateError::NotInitialized);
        }

        std::thread::sleep(self.delay);

        self.seen += 1;
        if self.fail_every > 0 && self.seen.is_multiple_of(self.fail_every) {
            return Err(TranslateError::Internal(format!(
                "(Имитирована ошибка во фрагменте {})",
                self.seen
            )));
        }

        Ok(format!("⟦Тестовый перевод⟧{text}"))
    }

    fn shutdown(&mut self) {
        self.initialized = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_before_init_is_rejected() {
        let mut t = MockTranslator::default();
        assert!(matches!(
            t.translate("hello"),
            Err(TranslateError::NotInitialized)
        ));
    }

    #[test]
    fn output_is_visibly_fake() {
        let mut t = MockTranslator::default();
        t.init(&TranslateConfig::default()).unwrap();
        let out = t.translate("hello world").unwrap();
        assert!(
            out.starts_with("⟦Тестовый перевод⟧"),
            "假译文应有明显标记：{out}"
        );
        assert!(out.contains("hello world"), "应保留原文以便比对");
    }

    #[test]
    fn failure_does_not_poison_the_engine() {
        let mut t = MockTranslator::new(Duration::ZERO, 2);
        t.init(&TranslateConfig::default()).unwrap();

        assert!(t.translate("first").is_ok());
        assert!(t.translate("second").is_err());
        assert!(t.translate("third").is_ok(), "失败后必须还能继续翻");
    }
}
