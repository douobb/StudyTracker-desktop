use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
};

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, Transaction};

use crate::{
    domain::settings::{AppSettings, SETTINGS_SCHEMA_VERSION},
    error::{AppError, AppResult},
    ports::repository::{SettingsRepository, UnitOfWork},
};

pub const DATABASE_SCHEMA_VERSION: u32 = 1;
const FOUNDATION_MIGRATION: &str = include_str!("../../../migrations/0001_foundation.sql");
const SETTINGS_KEY: &str = "app_settings";

pub struct SqliteStore {
    connection: Mutex<Connection>,
}

impl SqliteStore {
    pub fn open(path: impl AsRef<Path>) -> AppResult<Self> {
        let connection = Connection::open(path).map_err(|_| AppError::Database)?;
        Self::from_connection(connection)
    }

    pub fn in_memory() -> AppResult<Self> {
        let connection = Connection::open_in_memory().map_err(|_| AppError::Database)?;
        Self::from_connection(connection)
    }

    fn from_connection(mut connection: Connection) -> AppResult<Self> {
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA busy_timeout = 5000;",
            )
            .map_err(|_| AppError::Database)?;
        migrate(&mut connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn schema_version(&self) -> AppResult<u32> {
        let connection = self.lock()?;
        let version = connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
            .map_err(|_| AppError::Database)?;
        Ok(version)
    }

    pub fn transact<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction().map_err(|_| AppError::Database)?;
        let result = operation(&transaction)?;
        transaction.commit().map_err(|_| AppError::Database)?;
        Ok(result)
    }

    fn lock(&self) -> AppResult<MutexGuard<'_, Connection>> {
        self.connection.lock().map_err(|_| AppError::Database)
    }
}

impl SettingsRepository for SqliteStore {
    fn load(&self) -> AppResult<Option<AppSettings>> {
        let record = {
            let connection = self.lock()?;
            connection
                .query_row(
                    "SELECT value_json, schema_version FROM settings WHERE key = ?1",
                    [SETTINGS_KEY],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?)),
                )
                .optional()
                .map_err(|_| AppError::Database)?
        };
        let Some((json, schema_version)) = record else {
            return Ok(None);
        };
        let settings = AppSettings::from_persisted(schema_version, &json)?;
        if schema_version < SETTINGS_SCHEMA_VERSION {
            self.save(&settings)?;
        }
        Ok(Some(settings))
    }

    fn save(&self, settings: &AppSettings) -> AppResult<()> {
        settings.validate()?;
        let value = serde_json::to_string(settings).map_err(|_| AppError::Serialization)?;
        let now = Utc::now().to_rfc3339();
        self.transact(|transaction| {
            transaction
                .execute(
                    "INSERT INTO settings (key, value_json, schema_version, updated_at_utc)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(key) DO UPDATE SET
                       value_json = excluded.value_json,
                       schema_version = excluded.schema_version,
                       updated_at_utc = excluded.updated_at_utc",
                    (
                        SETTINGS_KEY,
                        value.as_str(),
                        SETTINGS_SCHEMA_VERSION,
                        now.as_str(),
                    ),
                )
                .map_err(|_| AppError::Database)?;
            Ok(())
        })
    }
}

impl UnitOfWork for SqliteStore {
    fn verify_integrity(&self) -> AppResult<()> {
        let connection = self.lock()?;
        let result: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .map_err(|_| AppError::Database)?;
        if result == "ok" {
            Ok(())
        } else {
            Err(AppError::Database)
        }
    }
}

fn migrate(connection: &mut Connection) -> AppResult<()> {
    let current = connection
        .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
        .map_err(|_| AppError::Database)?;
    if current > DATABASE_SCHEMA_VERSION {
        return Err(AppError::UnknownDatabaseVersion {
            found: current,
            supported: DATABASE_SCHEMA_VERSION,
        });
    }
    if current == DATABASE_SCHEMA_VERSION {
        return Ok(());
    }

    let transaction = connection.transaction().map_err(|_| AppError::Database)?;
    if current == 0 {
        transaction
            .execute_batch(FOUNDATION_MIGRATION)
            .map_err(|_| AppError::Database)?;
        transaction
            .pragma_update(None, "user_version", DATABASE_SCHEMA_VERSION)
            .map_err(|_| AppError::Database)?;
    }
    transaction.commit().map_err(|_| AppError::Database)
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{DATABASE_SCHEMA_VERSION, SETTINGS_KEY, SqliteStore, migrate};
    use crate::{
        domain::settings::{AppSettings, SETTINGS_SCHEMA_VERSION},
        error::AppError,
        ports::repository::{SettingsRepository, UnitOfWork},
    };

    #[test]
    fn 全新資料庫可遷移且重跑安全() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate(&mut connection).unwrap();
        migrate(&mut connection).unwrap();
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, DATABASE_SCHEMA_VERSION);
    }

    #[test]
    fn 拒絕未知的較新版本() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "user_version", DATABASE_SCHEMA_VERSION + 1)
            .unwrap();
        assert!(matches!(
            migrate(&mut connection),
            Err(AppError::UnknownDatabaseVersion { .. })
        ));
    }

    #[test]
    fn 舊設定讀取後自動升級保存() {
        let store = SqliteStore::in_memory().unwrap();
        {
            let connection = store.lock().unwrap();
            connection
                .execute(
                    "INSERT INTO settings (key, value_json, schema_version, updated_at_utc)
                     VALUES (?1, ?2, 0, '2026-07-23T00:00:00Z')",
                    (
                        SETTINGS_KEY,
                        r#"{"idleThresholdMinutes":45,"theme":"dark"}"#,
                    ),
                )
                .unwrap();
        }
        let settings = store.load().unwrap().unwrap();
        assert_eq!(settings.idle_threshold_minutes, Some(45));
        let connection = store.lock().unwrap();
        let version: u32 = connection
            .query_row(
                "SELECT schema_version FROM settings WHERE key = ?1",
                [SETTINGS_KEY],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, SETTINGS_SCHEMA_VERSION);
    }
    #[test]
    fn 設定可保存讀取並通過完整性檢查() {
        let store = SqliteStore::in_memory().unwrap();
        let expected = AppSettings::default();
        store.save(&expected).unwrap();
        assert_eq!(store.load().unwrap(), Some(expected));
        assert!(store.verify_integrity().is_ok());
    }

    #[test]
    fn 交易失敗時不留下半套資料() {
        let store = SqliteStore::in_memory().unwrap();
        let result: Result<(), AppError> = store.transact(|transaction| {
            transaction
                .execute(
                    "INSERT INTO settings (key, value_json, schema_version, updated_at_utc)
                     VALUES ('temporary', '{}', 1, '2026-07-23T00:00:00Z')",
                    [],
                )
                .map_err(|_| AppError::Database)?;
            Err(AppError::Database)
        });
        assert!(result.is_err());
        let connection = store.lock().unwrap();
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE key = 'temporary'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}
