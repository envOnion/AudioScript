pub mod local_nmt;
pub mod mock;
pub mod queue;

use crate::config::TranslateConfig;

#[derive(Debug, thiserror::Error)]
pub enum TranslateError {
    #[error("Движок перевода не инициализирован")]
    NotInitialized,

    #[error("Не удалось загрузить модель: {0}")]
    ModelLoad(String),

    #[error("В каталоге модели отсутствуют файлы: {0}")]
    MissingModelFile(String),

    #[error("Неподдерживаемое направление перевода: {from} → {to}")]
    UnsupportedPair { from: String, to: String },

    #[error("Некорректный перевод ({reason}); проверьте выбранное направление")]
    Degenerate { reason: String },

    #[error("Внутренняя ошибка движка: {0}")]
    Internal(String),
}

const UNK_MARKERS: [&str; 2] = ["<unk>", "⁇"];

const REPEAT_MIN_LEN: usize = 8;

const REPEAT_RATIO: f32 = 0.6;

const REPEAT_FLOOR: usize = 4;

const TRAILING_RUN: usize = 4;

pub fn degenerate_translation(text: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() {
        return Some("Пустой перевод".into());
    }
    if let Some(m) = UNK_MARKERS.iter().find(|m| t.contains(**m)) {
        return Some(format!("Содержит {m}"));
    }

    let chars: Vec<char> = t.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() < REPEAT_FLOOR {
        return None;
    }

    let mut counts: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
    for c in &chars {
        *counts.entry(*c).or_default() += 1;
    }
    let (top, n) = counts
        .iter()
        .map(|(c, n)| (*c, *n))
        .max_by_key(|(_, n)| *n)?;
    let ratio = n as f32 / chars.len() as f32;

    if n == chars.len() {
        return Some(format!("Повторяется один символ «{top}» ×{n}"));
    }

    if chars.len() >= REPEAT_MIN_LEN && ratio >= REPEAT_RATIO {
        return Some(format!(
            "Повторение: символ «{top}» занимает {:.0}%",
            ratio * 100.0
        ));
    }

    if let Some(last) = chars.last() {
        let trailing = chars.iter().rev().take_while(|c| *c == last).count();
        if trailing >= TRAILING_RUN {
            return Some(format!("Повторение в конце «{last}» ×{trailing}"));
        }
    }

    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranslateCaps {
    pub incremental: bool,
    pub offline: bool,
    pub typical_latency_ms: u32,
}

impl TranslateCaps {
    pub const LOCAL_NMT: Self = Self {
        incremental: false,
        offline: true,
        typical_latency_ms: 300,
    };
}

pub trait Translator: Send {
    fn init(&mut self, cfg: &TranslateConfig) -> Result<(), TranslateError>;

    fn capabilities(&self) -> TranslateCaps;

    fn translate(&mut self, text: &str) -> Result<String, TranslateError>;

    fn shutdown(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catches_unk_from_wrong_direction() {
        assert!(degenerate_translation("<unk>").is_some());
        assert!(degenerate_translation("他说 <unk> 和 <unk> 了").is_some());
    }

    #[test]
    fn catches_repetition_loops() {
        for s in [
            "来来来来来来",
            "的 的 的 的 的 的 的 的",
            "所以这里的关键洞见 关键是 我们需要谨慎地处理 长期间预算预算 。。。。。。。",
        ] {
            assert!(degenerate_translation(s).is_some(), "应判为退化：{s}");
        }
    }

    #[test]
    fn empty_output_is_degenerate() {
        assert!(degenerate_translation("").is_some());
        assert!(degenerate_translation("   ").is_some());
    }

    #[test]
    fn normal_translations_are_not_flagged() {
        for s in [
            "所以这里的关键洞察是我们需要处理延迟预算",
            "So the key insight here is that we need to handle the latency budget.",
            "让我来给你们讲讲架构如何将采集和推论分开。",
            "This is actually a really common mistake people make.",
            "The biggest problem with this programme is the delay.",
        ] {
            assert!(
                degenerate_translation(s).is_none(),
                "不该判为退化：{s}（{:?}）",
                degenerate_translation(s)
            );
        }
    }

    #[test]
    fn short_repeats_are_tolerated() {
        assert!(degenerate_translation("的的").is_none());
        assert!(degenerate_translation("嗯嗯").is_none());
    }

    #[test]
    fn natural_repetition_in_real_speech_survives() {
        assert!(degenerate_translation("这个真的很重要很重要").is_none());
    }
}
