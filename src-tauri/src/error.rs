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
    #[error("科目資料無效：{0}")]
    InvalidSubject(&'static str),
    #[error("找不到指定科目")]
    SubjectNotFound,
    #[error("科目已封存，不能建立新的學習工作")]
    SubjectArchived,
    #[error("資料已由其他操作更新")]
    RevisionConflict,
    #[error("任務資料無效：{0}")]
    InvalidTask(&'static str),
    #[error("找不到指定任務")]
    TaskNotFound,
    #[error("任務已封存")]
    TaskArchived,
    #[error("每日計畫資料無效：{0}")]
    InvalidDailyPlan(&'static str),
    #[error("找不到指定每日計畫")]
    DailyPlanNotFound,
    #[error("找不到指定每日計畫項目")]
    DailyPlanItemNotFound,
    #[error("每日計畫項目的科目與任務不一致")]
    DailyPlanReferenceMismatch,
    #[error("工作階段資料無效：{0}")]
    InvalidSession(&'static str),
    #[error("找不到指定工作階段")]
    SessionNotFound,
    #[error("已有進行中的工作階段")]
    ActiveSessionExists,
    #[error("工作階段時間與既有紀錄重疊")]
    SessionOverlap,
    #[error("目前狀態不允許此工作階段操作")]
    InvalidSessionTransition,
    #[error("工作階段的科目、任務或計畫項目不一致")]
    SessionReferenceMismatch,
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
            AppError::InvalidSubject(_) => ("INVALID_SUBJECT", "科目資料無效。", false),
            AppError::SubjectNotFound => ("SUBJECT_NOT_FOUND", "找不到指定科目。", false),
            AppError::SubjectArchived => (
                "SUBJECT_ARCHIVED",
                "已封存的科目不能建立新的學習工作。",
                false,
            ),
            AppError::RevisionConflict => {
                ("REVISION_CONFLICT", "資料已更新，請重新載入後再試。", true)
            }
            AppError::InvalidTask(_) => ("INVALID_TASK", "任務資料無效。", false),
            AppError::TaskNotFound => ("TASK_NOT_FOUND", "找不到指定任務。", false),
            AppError::TaskArchived => ("TASK_ARCHIVED", "已封存的任務無法執行此操作。", false),
            AppError::InvalidDailyPlan(_) => ("INVALID_DAILY_PLAN", "每日計畫資料無效。", false),
            AppError::DailyPlanNotFound => ("DAILY_PLAN_NOT_FOUND", "找不到指定每日計畫。", false),
            AppError::DailyPlanItemNotFound => (
                "DAILY_PLAN_ITEM_NOT_FOUND",
                "找不到指定每日計畫項目。",
                false,
            ),
            AppError::DailyPlanReferenceMismatch => (
                "DAILY_PLAN_REFERENCE_MISMATCH",
                "每日計畫項目的科目與任務不一致。",
                false,
            ),
            AppError::InvalidSession(_) => ("INVALID_SESSION", "工作階段資料無效。", false),
            AppError::SessionNotFound => ("SESSION_NOT_FOUND", "找不到指定工作階段。", false),
            AppError::ActiveSessionExists => {
                ("ACTIVE_SESSION_EXISTS", "已有進行中的工作階段。", false)
            }
            AppError::SessionOverlap => ("SESSION_OVERLAP", "工作階段時間與既有紀錄重疊。", false),
            AppError::InvalidSessionTransition => (
                "INVALID_SESSION_TRANSITION",
                "目前狀態不允許此工作階段操作。",
                false,
            ),
            AppError::SessionReferenceMismatch => (
                "SESSION_REFERENCE_MISMATCH",
                "工作階段的科目、任務或計畫項目不一致。",
                false,
            ),
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
