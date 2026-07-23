use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("資料庫操作失敗")]
    Database,
    #[error("資料格式無效")]
    Serialization,
    #[error("設定值無效：{0}")]
    InvalidSetting(&'static str),
    #[error("資料庫版本 {found} 高於應用程式支援版本 {supported}")]
    UnknownDatabaseVersion { found: u32, supported: u32 },
    #[error("背景服務已在執行")]
    RuntimeAlreadyRunning,
    #[error("背景服務操作失敗")]
    Runtime,
    #[error("無法判定系統時區")]
    TimeZone,
    #[error("時間區間無效")]
    InvalidTimeRange,
    #[error("應用程式初始化失敗")]
    Initialization,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    pub code: &'static str,
    pub message: &'static str,
    pub retryable: bool,
}

impl From<AppError> for IpcError {
    fn from(error: AppError) -> Self {
        let (code, message, retryable) = match error {
            AppError::Database => ("DATABASE_ERROR", "本機資料操作失敗。", true),
            AppError::Serialization => ("INVALID_DATA", "本機資料格式無效。", false),
            AppError::InvalidSetting(_) => ("INVALID_SETTING", "設定值不在允許範圍內。", false),
            AppError::UnknownDatabaseVersion { .. } => (
                "UNSUPPORTED_DATABASE_VERSION",
                "資料庫版本高於目前應用程式可支援的版本。",
                false,
            ),
            AppError::RuntimeAlreadyRunning => {
                ("RUNTIME_ALREADY_RUNNING", "背景服務已在執行。", false)
            }
            AppError::Runtime => ("RUNTIME_ERROR", "背景服務暫時無法使用。", true),
            AppError::TimeZone | AppError::InvalidTimeRange => {
                ("TIME_ERROR", "無法處理指定的時間資料。", false)
            }
            AppError::Initialization => ("INITIALIZATION_ERROR", "應用程式初始化失敗。", true),
        };
        Self {
            code,
            message,
            retryable,
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
