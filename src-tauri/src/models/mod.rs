pub mod catalog;
pub mod disk;
pub mod download;
pub mod extract;
pub mod install;
pub mod state;
pub mod status;

use serde::{Deserialize, Serialize};

pub const EVENT_MODEL_PROGRESS: &str = "models://progress";

pub(crate) const IO_CHUNK: usize = 64 * 1024;

#[typeshare::typeshare]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelKind {
    Asr,
    Mt,
}

impl ModelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Asr => "asr",
            Self::Mt => "mt",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("Неизвестная модель: {0}")]
    UnknownModel(String),

    #[error("Модель {0} пока недоступна для загрузки")]
    NotDownloadable(String),

    #[error("Модель {0} уже установлена. Для переустановки сначала удалите её")]
    AlreadyInstalled(String),

    #[error("Встроенные модели входят в состав приложения и не могут быть удалены")]
    Bundled,

    #[error("Перед удалением выберите другую модель")]
    Selected,

    #[error("Перед удалением модели остановите распознавание")]
    InUse,

    #[error("Загрузка модели уже выполняется")]
    JobInProgress,

    #[error("Ошибка загрузки: {0}")]
    Network(String),

    #[error("Ошибка загрузки: HTTP {0}")]
    Http(u16),

    #[error("Ошибка проверки: ожидалось {expected}, получено {actual}")]
    Checksum { expected: String, actual: String },

    #[error("Небезопасное содержимое архива: {0}")]
    UnsafeArchive(String),

    #[error("Ошибка распаковки: {0}")]
    Extract(String),

    #[error("Недостаточно места на диске: требуется около {} МБ, доступно около {} МБ", .need / 1048576, .available / 1048576)]
    DiskSpace { need: u64, available: u64 },

    #[error("Отсутствуют файлы модели: {0}")]
    Incomplete(String),

    #[error("Ошибка файловой операции: {0}")]
    Io(String),

    #[error("Загрузка отменена")]
    Cancelled,

    #[error("{0}")]
    Internal(String),
}

impl From<std::io::Error> for ModelError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
