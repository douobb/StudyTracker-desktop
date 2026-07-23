use std::sync::Arc;

use crate::{
    domain::settings::AppSettings, error::AppResult, ports::repository::SettingsRepository,
};

pub struct SettingsService {
    repository: Arc<dyn SettingsRepository>,
}

impl SettingsService {
    pub fn new(repository: Arc<dyn SettingsRepository>) -> Self {
        Self { repository }
    }

    pub fn load_or_create_defaults(&self) -> AppResult<AppSettings> {
        if let Some(settings) = self.repository.load()? {
            settings.validate()?;
            return Ok(settings);
        }
        let defaults = AppSettings::default();
        self.repository.save(&defaults)?;
        Ok(defaults)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::SettingsService;
    use crate::{
        domain::settings::AppSettings, error::AppResult, ports::repository::SettingsRepository,
    };

    #[derive(Default)]
    struct MemorySettingsRepository {
        value: Mutex<Option<AppSettings>>,
    }

    impl SettingsRepository for MemorySettingsRepository {
        fn load(&self) -> AppResult<Option<AppSettings>> {
            Ok(self.value.lock().unwrap().clone())
        }

        fn save(&self, settings: &AppSettings) -> AppResult<()> {
            *self.value.lock().unwrap() = Some(settings.clone());
            Ok(())
        }
    }

    #[test]
    fn 首次啟動建立預設設定且之後重用() {
        let repository = Arc::new(MemorySettingsRepository::default());
        let service = SettingsService::new(repository.clone());
        let first = service.load_or_create_defaults().unwrap();
        let second = service.load_or_create_defaults().unwrap();
        assert_eq!(first, AppSettings::default());
        assert_eq!(second, first);
    }
}
