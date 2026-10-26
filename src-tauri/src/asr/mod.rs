pub mod segment;
pub mod streaming;

use crate::config::AsrConfig;
use crate::transcript::TranscriptSegment;

pub const TARGET_SAMPLE_RATE: u32 = 16_000;

pub mod latin_case;

#[derive(Debug, thiserror::Error)]
pub enum AsrError {
    #[error("Движок не инициализирован")]
    NotInitialized,

    #[error("Некорректные настройки: {0}")]
    InvalidConfig(String),

    #[error("Не удалось загрузить модель: {0}")]
    ModelLoad(String),

    #[error("В каталоге модели отсутствуют файлы: {0}")]
    MissingModelFile(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineCaps {
    pub streaming: bool,
    pub produces_partial: bool,
    pub requires_full_utterance: bool,
    pub supports_endpoint: bool,
}

impl EngineCaps {
    pub const STREAMING_TRANSDUCER: Self = Self {
        streaming: true,
        produces_partial: true,
        requires_full_utterance: false,
        supports_endpoint: true,
    };
}

pub trait AsrEngine: Send {
    fn init(&mut self, cfg: &AsrConfig) -> Result<(), AsrError>;

    fn capabilities(&self) -> EngineCaps;

    fn feed_pcm(&mut self, pcm: &[f32]) -> Result<Vec<TranscriptSegment>, AsrError>;

    fn flush(&mut self) -> Result<Vec<TranscriptSegment>, AsrError>;

    fn shutdown(&mut self);
}
