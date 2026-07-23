use crate::{domain::settings::AppSettings, error::AppResult};

pub trait SettingsRepository: Send + Sync {
    fn load(&self) -> AppResult<Option<AppSettings>>;
    fn save(&self, settings: &AppSettings) -> AppResult<()>;
}

pub trait UnitOfWork: Send + Sync {
    fn verify_integrity(&self) -> AppResult<()>;
}
