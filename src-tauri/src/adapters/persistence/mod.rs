use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, Row, Transaction};
use uuid::Uuid;

use crate::{
    domain::{
        settings::{AppSettings, SETTINGS_SCHEMA_VERSION},
        study_task::{StudyTask, StudyTaskSnapshot, TaskStatus},
        subject::{Subject, SubjectSnapshot},
    },
    error::{AppError, AppResult},
    ports::repository::{SettingsRepository, SubjectRepository, TaskRepository, UnitOfWork},
};

mod daily_plans;
mod sessions;

pub const DATABASE_SCHEMA_VERSION: u32 = 1;
const FOUNDATION_MIGRATION: &str = include_str!("../../../migrations/0001_foundation.sql");
const SETTINGS_KEY: &str = "app_settings";

struct SubjectRecord {
    id: String,
    name: String,
    sort_order: i64,
    archived_at_utc: Option<String>,
    created_at_utc: String,
    updated_at_utc: String,
    source_device_id: String,
    revision: i64,
    deleted_at_utc: Option<String>,
}

impl SubjectRecord {
    fn into_subject(self) -> AppResult<Subject> {
        Subject::restore(SubjectSnapshot {
            id: parse_uuid(&self.id)?,
            name: self.name,
            sort_order: self.sort_order,
            archived_at_utc: self
                .archived_at_utc
                .as_deref()
                .map(parse_timestamp)
                .transpose()?,
            created_at_utc: parse_timestamp(&self.created_at_utc)?,
            updated_at_utc: parse_timestamp(&self.updated_at_utc)?,
            source_device_id: parse_uuid(&self.source_device_id)?,
            revision: u64::try_from(self.revision).map_err(|_| AppError::Serialization)?,
            deleted_at_utc: self
                .deleted_at_utc
                .as_deref()
                .map(parse_timestamp)
                .transpose()?,
        })
    }
}

struct TaskRecord {
    id: String,
    subject_id: String,
    title: String,
    status: String,
    created_at_utc: String,
    updated_at_utc: String,
    source_device_id: String,
    revision: i64,
    deleted_at_utc: Option<String>,
}

impl TaskRecord {
    fn into_task(self) -> AppResult<StudyTask> {
        StudyTask::restore(StudyTaskSnapshot {
            id: parse_uuid(&self.id)?,
            subject_id: parse_uuid(&self.subject_id)?,
            title: self.title,
            status: TaskStatus::from_persisted(&self.status)?,
            created_at_utc: parse_timestamp(&self.created_at_utc)?,
            updated_at_utc: parse_timestamp(&self.updated_at_utc)?,
            source_device_id: parse_uuid(&self.source_device_id)?,
            revision: u64::try_from(self.revision).map_err(|_| AppError::Serialization)?,
            deleted_at_utc: self
                .deleted_at_utc
                .as_deref()
                .map(parse_timestamp)
                .transpose()?,
        })
    }
}

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

impl SubjectRepository for SqliteStore {
    fn insert(&self, subject: &Subject) -> AppResult<()> {
        self.transact(|transaction| {
            transaction
                .execute(
                    "INSERT INTO subjects (
                        id, name, sort_order, archived_at_utc, created_at_utc, updated_at_utc,
                        source_device_id, revision, deleted_at_utc
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    (
                        subject.id().to_string(),
                        subject.name(),
                        subject.sort_order(),
                        format_timestamp(subject.archived_at_utc()),
                        subject.created_at_utc().to_rfc3339(),
                        subject.updated_at_utc().to_rfc3339(),
                        subject.source_device_id().to_string(),
                        revision_to_sql(subject.revision())?,
                        format_timestamp(subject.deleted_at_utc()),
                    ),
                )
                .map_err(|_| AppError::Database)?;
            Ok(())
        })
    }

    fn get(&self, id: Uuid) -> AppResult<Option<Subject>> {
        let record = {
            let connection = self.lock()?;
            connection
                .query_row(
                    "SELECT id, name, sort_order, archived_at_utc, created_at_utc,
                            updated_at_utc, source_device_id, revision, deleted_at_utc
                     FROM subjects
                     WHERE id = ?1 AND deleted_at_utc IS NULL",
                    [id.to_string()],
                    read_subject_record,
                )
                .optional()
                .map_err(|_| AppError::Database)?
        };
        record.map(SubjectRecord::into_subject).transpose()
    }

    fn list(&self, include_archived: bool) -> AppResult<Vec<Subject>> {
        let records = {
            let connection = self.lock()?;
            let mut statement = connection
                .prepare(
                    "SELECT id, name, sort_order, archived_at_utc, created_at_utc,
                            updated_at_utc, source_device_id, revision, deleted_at_utc
                     FROM subjects
                     WHERE deleted_at_utc IS NULL
                       AND (?1 = 1 OR archived_at_utc IS NULL)
                     ORDER BY sort_order ASC, name COLLATE NOCASE ASC, id ASC",
                )
                .map_err(|_| AppError::Database)?;
            statement
                .query_map([include_archived], read_subject_record)
                .map_err(|_| AppError::Database)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| AppError::Database)?
        };
        records
            .into_iter()
            .map(SubjectRecord::into_subject)
            .collect()
    }

    fn update(&self, subject: &Subject, expected_revision: u64) -> AppResult<()> {
        if subject.revision()
            != expected_revision
                .checked_add(1)
                .ok_or(AppError::RevisionConflict)?
        {
            return Err(AppError::RevisionConflict);
        }
        self.transact(|transaction| {
            let changed = transaction
                .execute(
                    "UPDATE subjects SET
                        name = ?1,
                        sort_order = ?2,
                        archived_at_utc = ?3,
                        updated_at_utc = ?4,
                        revision = ?5
                     WHERE id = ?6 AND revision = ?7 AND deleted_at_utc IS NULL",
                    (
                        subject.name(),
                        subject.sort_order(),
                        format_timestamp(subject.archived_at_utc()),
                        subject.updated_at_utc().to_rfc3339(),
                        revision_to_sql(subject.revision())?,
                        subject.id().to_string(),
                        revision_to_sql(expected_revision)?,
                    ),
                )
                .map_err(|_| AppError::Database)?;
            if changed == 0 {
                let exists = transaction
                    .query_row(
                        "SELECT 1 FROM subjects WHERE id = ?1 AND deleted_at_utc IS NULL",
                        [subject.id().to_string()],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(|_| AppError::Database)?;
                return Err(if exists.is_some() {
                    AppError::RevisionConflict
                } else {
                    AppError::SubjectNotFound
                });
            }
            transaction
                .execute(
                    "INSERT INTO audit_revisions (
                        id, entity_kind, entity_id, previous_revision, new_revision,
                        change_summary_json, changed_at_utc, source_device_id
                     ) VALUES (?1, 'subject', ?2, ?3, ?4, ?5, ?6, ?7)",
                    (
                        Uuid::now_v7().to_string(),
                        subject.id().to_string(),
                        revision_to_sql(expected_revision)?,
                        revision_to_sql(subject.revision())?,
                        r#"{"operation":"subject_update"}"#,
                        subject.updated_at_utc().to_rfc3339(),
                        subject.source_device_id().to_string(),
                    ),
                )
                .map_err(|_| AppError::Database)?;
            Ok(())
        })
    }
}

impl TaskRepository for SqliteStore {
    fn insert(&self, task: &StudyTask) -> AppResult<()> {
        self.transact(|transaction| {
            let inserted = transaction
                .execute(
                    "INSERT INTO tasks (
                        id, subject_id, title, status, created_at_utc, updated_at_utc,
                        source_device_id, revision, deleted_at_utc
                     )
                     SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9
                     WHERE EXISTS (
                         SELECT 1 FROM subjects
                         WHERE id = ?2 AND archived_at_utc IS NULL AND deleted_at_utc IS NULL
                     )",
                    (
                        task.id().to_string(),
                        task.subject_id().to_string(),
                        task.title(),
                        task.status().as_str(),
                        task.created_at_utc().to_rfc3339(),
                        task.updated_at_utc().to_rfc3339(),
                        task.source_device_id().to_string(),
                        revision_to_sql(task.revision())?,
                        format_timestamp(task.deleted_at_utc()),
                    ),
                )
                .map_err(|_| AppError::Database)?;
            if inserted == 0 {
                let subject_exists = transaction
                    .query_row(
                        "SELECT 1 FROM subjects WHERE id = ?1 AND deleted_at_utc IS NULL",
                        [task.subject_id().to_string()],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(|_| AppError::Database)?;
                return Err(if subject_exists.is_some() {
                    AppError::SubjectArchived
                } else {
                    AppError::SubjectNotFound
                });
            }
            Ok(())
        })
    }

    fn get(&self, id: Uuid) -> AppResult<Option<StudyTask>> {
        let record = {
            let connection = self.lock()?;
            connection
                .query_row(
                    "SELECT id, subject_id, title, status, created_at_utc, updated_at_utc,
                            source_device_id, revision, deleted_at_utc
                     FROM tasks
                     WHERE id = ?1 AND deleted_at_utc IS NULL",
                    [id.to_string()],
                    read_task_record,
                )
                .optional()
                .map_err(|_| AppError::Database)?
        };
        record.map(TaskRecord::into_task).transpose()
    }

    fn list_by_subject(
        &self,
        subject_id: Uuid,
        include_archived: bool,
    ) -> AppResult<Vec<StudyTask>> {
        let records = {
            let connection = self.lock()?;
            let mut statement = connection
                .prepare(
                    "SELECT id, subject_id, title, status, created_at_utc, updated_at_utc,
                            source_device_id, revision, deleted_at_utc
                     FROM tasks
                     WHERE subject_id = ?1
                       AND deleted_at_utc IS NULL
                       AND (?2 = 1 OR status != 'archived')
                     ORDER BY created_at_utc ASC, title COLLATE NOCASE ASC, id ASC",
                )
                .map_err(|_| AppError::Database)?;
            statement
                .query_map((subject_id.to_string(), include_archived), read_task_record)
                .map_err(|_| AppError::Database)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| AppError::Database)?
        };
        records.into_iter().map(TaskRecord::into_task).collect()
    }

    fn update(&self, task: &StudyTask, expected_revision: u64) -> AppResult<()> {
        if task.revision()
            != expected_revision
                .checked_add(1)
                .ok_or(AppError::RevisionConflict)?
        {
            return Err(AppError::RevisionConflict);
        }
        self.transact(|transaction| {
            let changed = transaction
                .execute(
                    "UPDATE tasks SET
                        title = ?1,
                        status = ?2,
                        updated_at_utc = ?3,
                        revision = ?4
                     WHERE id = ?5
                       AND subject_id = ?6
                       AND revision = ?7
                       AND deleted_at_utc IS NULL",
                    (
                        task.title(),
                        task.status().as_str(),
                        task.updated_at_utc().to_rfc3339(),
                        revision_to_sql(task.revision())?,
                        task.id().to_string(),
                        task.subject_id().to_string(),
                        revision_to_sql(expected_revision)?,
                    ),
                )
                .map_err(|_| AppError::Database)?;
            if changed == 0 {
                let stored = transaction
                    .query_row(
                        "SELECT subject_id FROM tasks WHERE id = ?1 AND deleted_at_utc IS NULL",
                        [task.id().to_string()],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(|_| AppError::Database)?;
                return Err(match stored {
                    None => AppError::TaskNotFound,
                    Some(subject_id) if subject_id != task.subject_id().to_string() => {
                        AppError::InvalidTask("subject_id")
                    }
                    Some(_) => AppError::RevisionConflict,
                });
            }
            transaction
                .execute(
                    "INSERT INTO audit_revisions (
                        id, entity_kind, entity_id, previous_revision, new_revision,
                        change_summary_json, changed_at_utc, source_device_id
                     ) VALUES (?1, 'task', ?2, ?3, ?4, ?5, ?6, ?7)",
                    (
                        Uuid::now_v7().to_string(),
                        task.id().to_string(),
                        revision_to_sql(expected_revision)?,
                        revision_to_sql(task.revision())?,
                        r#"{"operation":"task_update"}"#,
                        task.updated_at_utc().to_rfc3339(),
                        task.source_device_id().to_string(),
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

fn read_subject_record(row: &Row<'_>) -> rusqlite::Result<SubjectRecord> {
    Ok(SubjectRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        sort_order: row.get(2)?,
        archived_at_utc: row.get(3)?,
        created_at_utc: row.get(4)?,
        updated_at_utc: row.get(5)?,
        source_device_id: row.get(6)?,
        revision: row.get(7)?,
        deleted_at_utc: row.get(8)?,
    })
}

fn read_task_record(row: &Row<'_>) -> rusqlite::Result<TaskRecord> {
    Ok(TaskRecord {
        id: row.get(0)?,
        subject_id: row.get(1)?,
        title: row.get(2)?,
        status: row.get(3)?,
        created_at_utc: row.get(4)?,
        updated_at_utc: row.get(5)?,
        source_device_id: row.get(6)?,
        revision: row.get(7)?,
        deleted_at_utc: row.get(8)?,
    })
}

fn parse_uuid(value: &str) -> AppResult<Uuid> {
    Uuid::parse_str(value).map_err(|_| AppError::Serialization)
}

fn parse_timestamp(value: &str) -> AppResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|_| AppError::Serialization)
}

fn format_timestamp(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(|timestamp| timestamp.to_rfc3339())
}

fn revision_to_sql(revision: u64) -> AppResult<i64> {
    i64::try_from(revision).map_err(|_| AppError::Serialization)
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use rusqlite::Connection;
    use uuid::Uuid;

    use super::{DATABASE_SCHEMA_VERSION, SETTINGS_KEY, SqliteStore, migrate};
    use crate::{
        domain::{
            settings::{AppSettings, SETTINGS_SCHEMA_VERSION},
            study_task::{StudyTask, StudyTaskSnapshot, TaskStatus},
            subject::Subject,
        },
        error::AppError,
        ports::repository::{SettingsRepository, SubjectRepository, TaskRepository, UnitOfWork},
    };

    fn insert_device(store: &SqliteStore, id: Uuid) {
        let connection = store.lock().unwrap();
        connection
            .execute(
                "INSERT INTO devices (
                    id, display_name, created_at_utc, updated_at_utc, revision
                 ) VALUES (?1, '測試裝置', '2026-07-23T08:00:00Z', '2026-07-23T08:00:00Z', 1)",
                [id.to_string()],
            )
            .unwrap();
    }

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

    #[test]
    fn 科目可保存排序封存且保留歷史() {
        let store = SqliteStore::in_memory().unwrap();
        let device_id = Uuid::now_v7();
        insert_device(&store, device_id);
        let created_at = Utc.with_ymd_and_hms(2026, 7, 23, 8, 0, 0).unwrap();
        let mut mathematics =
            Subject::create(Uuid::now_v7(), "數學", 10, created_at, device_id).unwrap();
        let english = Subject::create(Uuid::now_v7(), "英文", 0, created_at, device_id).unwrap();
        SubjectRepository::insert(&store, &mathematics).unwrap();
        SubjectRepository::insert(&store, &english).unwrap();
        assert_eq!(store.list(false).unwrap()[0].id(), english.id());

        let previous_revision = mathematics.revision();
        mathematics
            .reorder(-1, created_at + Duration::minutes(1))
            .unwrap();
        SubjectRepository::update(&store, &mathematics, previous_revision).unwrap();
        assert_eq!(store.list(false).unwrap()[0].id(), mathematics.id());

        let previous_revision = mathematics.revision();
        mathematics
            .archive(created_at + Duration::minutes(2))
            .unwrap();
        SubjectRepository::update(&store, &mathematics, previous_revision).unwrap();
        assert!(
            store
                .list(false)
                .unwrap()
                .iter()
                .all(|item| item.id() != mathematics.id())
        );
        assert_eq!(
            SubjectRepository::get(&store, mathematics.id())
                .unwrap()
                .unwrap()
                .archived_at_utc(),
            mathematics.archived_at_utc()
        );
        assert!(
            store
                .list(true)
                .unwrap()
                .iter()
                .any(|item| item.id() == mathematics.id())
        );

        let connection = store.lock().unwrap();
        let audit_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_revisions WHERE entity_kind = 'subject' AND entity_id = ?1",
                [mathematics.id().to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(audit_count, 2);
    }

    #[test]
    fn 科目更新使用樂觀版本避免覆寫() {
        let store = SqliteStore::in_memory().unwrap();
        let device_id = Uuid::now_v7();
        insert_device(&store, device_id);
        let created_at = Utc.with_ymd_and_hms(2026, 7, 23, 8, 0, 0).unwrap();
        let subject = Subject::create(Uuid::now_v7(), "數學", 0, created_at, device_id).unwrap();
        SubjectRepository::insert(&store, &subject).unwrap();

        let mut first_update = subject.clone();
        first_update
            .rename("進階數學", created_at + Duration::minutes(1))
            .unwrap();
        let mut stale_update = subject;
        stale_update
            .rename("基礎數學", created_at + Duration::minutes(1))
            .unwrap();
        SubjectRepository::update(&store, &first_update, 1).unwrap();
        assert!(matches!(
            SubjectRepository::update(&store, &stale_update, 1),
            Err(AppError::RevisionConflict)
        ));
        assert_eq!(
            SubjectRepository::get(&store, first_update.id())
                .unwrap()
                .unwrap()
                .name(),
            "進階數學"
        );
    }

    #[test]
    fn 任務可保存完成重開封存並保留歷史() {
        let store = SqliteStore::in_memory().unwrap();
        let device_id = Uuid::now_v7();
        insert_device(&store, device_id);
        let now = Utc.with_ymd_and_hms(2026, 7, 23, 8, 0, 0).unwrap();
        let subject = Subject::create(Uuid::now_v7(), "數學", 0, now, device_id).unwrap();
        SubjectRepository::insert(&store, &subject).unwrap();
        let mut task =
            StudyTask::create(Uuid::now_v7(), subject.id(), "練習第一章", now, device_id).unwrap();
        TaskRepository::insert(&store, &task).unwrap();

        let revision = task.revision();
        task.complete(now + Duration::minutes(1)).unwrap();
        TaskRepository::update(&store, &task, revision).unwrap();
        assert_eq!(
            TaskRepository::get(&store, task.id())
                .unwrap()
                .unwrap()
                .status(),
            TaskStatus::Completed
        );
        let revision = task.revision();
        task.reopen(now + Duration::minutes(2)).unwrap();
        TaskRepository::update(&store, &task, revision).unwrap();
        let revision = task.revision();
        task.archive(now + Duration::minutes(3)).unwrap();
        TaskRepository::update(&store, &task, revision).unwrap();

        assert!(
            TaskRepository::list_by_subject(&store, subject.id(), false)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            TaskRepository::list_by_subject(&store, subject.id(), true).unwrap()[0].id(),
            task.id()
        );
        let connection = store.lock().unwrap();
        let audit_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_revisions WHERE entity_kind = 'task' AND entity_id = ?1",
                [task.id().to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(audit_count, 3);
    }

    #[test]
    fn 任務必須隸屬現有科目且不可在更新時更換科目() {
        let store = SqliteStore::in_memory().unwrap();
        let device_id = Uuid::now_v7();
        insert_device(&store, device_id);
        let now = Utc.with_ymd_and_hms(2026, 7, 23, 8, 0, 0).unwrap();
        let missing_subject_task =
            StudyTask::create(Uuid::now_v7(), Uuid::now_v7(), "無效任務", now, device_id).unwrap();
        assert!(matches!(
            TaskRepository::insert(&store, &missing_subject_task),
            Err(AppError::SubjectNotFound)
        ));

        let subject = Subject::create(Uuid::now_v7(), "數學", 0, now, device_id).unwrap();
        let other_subject = Subject::create(Uuid::now_v7(), "英文", 1, now, device_id).unwrap();
        SubjectRepository::insert(&store, &subject).unwrap();
        SubjectRepository::insert(&store, &other_subject).unwrap();
        let task =
            StudyTask::create(Uuid::now_v7(), subject.id(), "練習第一章", now, device_id).unwrap();
        TaskRepository::insert(&store, &task).unwrap();
        let mut forged = StudyTask::restore(StudyTaskSnapshot {
            id: task.id(),
            subject_id: other_subject.id(),
            title: "偷換科目".to_owned(),
            status: TaskStatus::Open,
            created_at_utc: task.created_at_utc(),
            updated_at_utc: now + Duration::minutes(1),
            source_device_id: task.source_device_id(),
            revision: task.revision() + 1,
            deleted_at_utc: None,
        })
        .unwrap();
        assert!(matches!(
            TaskRepository::update(&store, &forged, task.revision()),
            Err(AppError::InvalidTask("subject_id"))
        ));
        forged = TaskRepository::get(&store, task.id()).unwrap().unwrap();
        assert_eq!(forged.subject_id(), subject.id());
    }

    #[test]
    fn 資料層原子拒絕在封存科目建立任務() {
        let store = SqliteStore::in_memory().unwrap();
        let device_id = Uuid::now_v7();
        insert_device(&store, device_id);
        let now = Utc.with_ymd_and_hms(2026, 7, 23, 8, 0, 0).unwrap();
        let mut subject = Subject::create(Uuid::now_v7(), "數學", 0, now, device_id).unwrap();
        SubjectRepository::insert(&store, &subject).unwrap();
        let revision = subject.revision();
        subject.archive(now + Duration::minutes(1)).unwrap();
        SubjectRepository::update(&store, &subject, revision).unwrap();
        let task = StudyTask::create(
            Uuid::now_v7(),
            subject.id(),
            "封存後建立",
            now + Duration::minutes(2),
            device_id,
        )
        .unwrap();
        assert!(matches!(
            TaskRepository::insert(&store, &task),
            Err(AppError::SubjectArchived)
        ));
        assert!(TaskRepository::get(&store, task.id()).unwrap().is_none());
    }

    #[test]
    fn 同一任務可被不同日期的計畫項目參照() {
        let store = SqliteStore::in_memory().unwrap();
        let device_id = Uuid::now_v7();
        insert_device(&store, device_id);
        let now = Utc.with_ymd_and_hms(2026, 7, 23, 8, 0, 0).unwrap();
        let subject = Subject::create(Uuid::now_v7(), "數學", 0, now, device_id).unwrap();
        SubjectRepository::insert(&store, &subject).unwrap();
        let task =
            StudyTask::create(Uuid::now_v7(), subject.id(), "練習第一章", now, device_id).unwrap();
        TaskRepository::insert(&store, &task).unwrap();

        store
            .transact(|transaction| {
                for (date, order) in [("2026-07-23", 0), ("2026-07-24", 1)] {
                    let plan_id = Uuid::now_v7();
                    transaction
                        .execute(
                            "INSERT INTO daily_plans (
                                id, local_date, time_zone_id, created_at_utc, updated_at_utc,
                                source_device_id, revision
                             ) VALUES (?1, ?2, 'Asia/Taipei', ?3, ?3, ?4, 1)",
                            (
                                plan_id.to_string(),
                                date,
                                now.to_rfc3339(),
                                device_id.to_string(),
                            ),
                        )
                        .map_err(|_| AppError::Database)?;
                    transaction
                        .execute(
                            "INSERT INTO daily_plan_items (
                                id, daily_plan_id, subject_id, task_id, title, status,
                                sort_order, created_at_utc, updated_at_utc, source_device_id, revision
                             ) VALUES (?1, ?2, ?3, ?4, ?5, 'planned', ?6, ?7, ?7, ?8, 1)",
                            (
                                Uuid::now_v7().to_string(),
                                plan_id.to_string(),
                                subject.id().to_string(),
                                task.id().to_string(),
                                task.title(),
                                order,
                                now.to_rfc3339(),
                                device_id.to_string(),
                            ),
                        )
                        .map_err(|_| AppError::Database)?;
                }
                Ok(())
            })
            .unwrap();

        let connection = store.lock().unwrap();
        let date_count: i64 = connection
            .query_row(
                "SELECT COUNT(DISTINCT daily_plans.local_date)
                 FROM daily_plan_items
                 JOIN daily_plans ON daily_plans.id = daily_plan_items.daily_plan_id
                 WHERE daily_plan_items.task_id = ?1",
                [task.id().to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(date_count, 2);
    }
}
