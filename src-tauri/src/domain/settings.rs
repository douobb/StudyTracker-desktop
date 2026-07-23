use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub const SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeekStart {
    Monday,
    Sunday,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub schema_version: u32,
    pub idle_threshold_minutes: Option<u16>,
    pub activity_tracking_enabled: bool,
    pub window_title_read_enabled: bool,
    pub window_title_save_enabled: bool,
    pub theme: ThemePreference,
    pub week_start: WeekStart,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacySettingsV0 {
    #[serde(default = "default_idle_threshold")]
    idle_threshold_minutes: Option<u16>,
    #[serde(default = "enabled_by_default")]
    activity_tracking_enabled: bool,
    #[serde(default)]
    theme: ThemePreference,
}

const fn default_idle_threshold() -> Option<u16> {
    Some(30)
}

const fn enabled_by_default() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            idle_threshold_minutes: Some(30),
            activity_tracking_enabled: true,
            window_title_read_enabled: false,
            window_title_save_enabled: false,
            theme: ThemePreference::System,
            week_start: WeekStart::Monday,
        }
    }
}

impl AppSettings {
    pub fn from_persisted(schema_version: u32, json: &str) -> AppResult<Self> {
        let settings = match schema_version {
            0 => {
                let legacy: LegacySettingsV0 =
                    serde_json::from_str(json).map_err(|_| AppError::Serialization)?;
                Self {
                    schema_version: SETTINGS_SCHEMA_VERSION,
                    idle_threshold_minutes: legacy.idle_threshold_minutes,
                    activity_tracking_enabled: legacy.activity_tracking_enabled,
                    window_title_read_enabled: false,
                    window_title_save_enabled: false,
                    theme: legacy.theme,
                    week_start: WeekStart::Monday,
                }
            }
            SETTINGS_SCHEMA_VERSION => {
                serde_json::from_str(json).map_err(|_| AppError::Serialization)?
            }
            _ => return Err(AppError::InvalidSetting("schema_version")),
        };
        settings.validate()?;
        Ok(settings)
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.schema_version != SETTINGS_SCHEMA_VERSION {
            return Err(AppError::InvalidSetting("schema_version"));
        }
        if let Some(minutes) = self.idle_threshold_minutes
            && !(1..=120).contains(&minutes)
        {
            return Err(AppError::InvalidSetting("idle_threshold_minutes"));
        }
        if self.window_title_save_enabled && !self.window_title_read_enabled {
            return Err(AppError::InvalidSetting("window_title_save_enabled"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{AppSettings, SETTINGS_SCHEMA_VERSION, ThemePreference, WeekStart};

    #[test]
    fn 預設閒置門檻為三十分鐘() {
        let settings = AppSettings::default();
        assert_eq!(settings.idle_threshold_minutes, Some(30));
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn 可停用閒置判定() {
        let settings = AppSettings {
            idle_threshold_minutes: None,
            ..AppSettings::default()
        };
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn 舊版設定可升級並補上隱私安全預設值() {
        let migrated = AppSettings::from_persisted(
            0,
            r#"{"idleThresholdMinutes":45,"activityTrackingEnabled":false,"theme":"dark"}"#,
        )
        .unwrap();
        assert_eq!(migrated.schema_version, SETTINGS_SCHEMA_VERSION);
        assert_eq!(migrated.idle_threshold_minutes, Some(45));
        assert!(!migrated.activity_tracking_enabled);
        assert!(!migrated.window_title_read_enabled);
        assert!(!migrated.window_title_save_enabled);
        assert_eq!(migrated.theme, ThemePreference::Dark);
        assert_eq!(migrated.week_start, WeekStart::Monday);
    }

    #[test]
    fn 拒絕範圍外門檻與未讀取卻保存標題() {
        let invalid_threshold = AppSettings {
            idle_threshold_minutes: Some(121),
            ..AppSettings::default()
        };
        assert!(invalid_threshold.validate().is_err());

        let invalid_consent = AppSettings {
            window_title_save_enabled: true,
            ..AppSettings::default()
        };
        assert!(invalid_consent.validate().is_err());
    }
}
