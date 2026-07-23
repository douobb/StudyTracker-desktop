use chrono_tz::Tz;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use uuid::Uuid;

use super::{SqliteStore, format_timestamp, parse_timestamp, parse_uuid, revision_to_sql};
use crate::{
    domain::{
        daily_plan::{DailyPlanItem, DailyPlanItemStatus},
        session::{
            SegmentKind, Session, SessionSegment, SessionSegmentSnapshot, SessionSnapshot,
            SessionState, TimerMode,
        },
    },
    error::{AppError, AppResult},
    ports::repository::SessionRepository,
};

struct SessionRecord {
    id: String,
    subject_id: String,
    task_id: Option<String>,
    daily_plan_item_id: Option<String>,
    timer_mode: String,
    state: String,
    started_at_utc: String,
    ended_at_utc: Option<String>,
    time_zone_id: String,
    timer_config_json: String,
    recovery_checkpoint_utc: String,
    created_at_utc: String,
    updated_at_utc: String,
    source_device_id: String,
    revision: i64,
    deleted_at_utc: Option<String>,
}

impl SessionRecord {
    fn into_session(self, segments: Vec<SessionSegment>) -> AppResult<Session> {
        Session::restore(
            SessionSnapshot {
                id: parse_uuid(&self.id)?,
                subject_id: parse_uuid(&self.subject_id)?,
                task_id: self.task_id.as_deref().map(parse_uuid).transpose()?,
                daily_plan_item_id: self
                    .daily_plan_item_id
                    .as_deref()
                    .map(parse_uuid)
                    .transpose()?,
                timer_mode: TimerMode::from_persisted(&self.timer_mode)?,
                state: SessionState::from_persisted(&self.state)?,
                started_at_utc: parse_timestamp(&self.started_at_utc)?,
                ended_at_utc: self
                    .ended_at_utc
                    .as_deref()
                    .map(parse_timestamp)
                    .transpose()?,
                time_zone: self
                    .time_zone_id
                    .parse::<Tz>()
                    .map_err(|_| AppError::Serialization)?,
                timer_config_json: self.timer_config_json,
                recovery_checkpoint_utc: parse_timestamp(&self.recovery_checkpoint_utc)?,
                created_at_utc: parse_timestamp(&self.created_at_utc)?,
                updated_at_utc: parse_timestamp(&self.updated_at_utc)?,
                source_device_id: parse_uuid(&self.source_device_id)?,
                revision: u64::try_from(self.revision).map_err(|_| AppError::Serialization)?,
                deleted_at_utc: self
                    .deleted_at_utc
                    .as_deref()
                    .map(parse_timestamp)
                    .transpose()?,
            },
            segments,
        )
    }
}

struct SegmentRecord {
    id: String,
    session_id: String,
    kind: String,
    started_at_utc: String,
    ended_at_utc: Option<String>,
    pending_reason: Option<String>,
    created_at_utc: String,
    updated_at_utc: String,
    source_device_id: String,
    revision: i64,
    deleted_at_utc: Option<String>,
}

impl SegmentRecord {
    fn into_segment(self) -> AppResult<SessionSegment> {
        SessionSegment::restore(SessionSegmentSnapshot {
            id: parse_uuid(&self.id)?,
            session_id: parse_uuid(&self.session_id)?,
            kind: SegmentKind::from_persisted(&self.kind)?,
            started_at_utc: parse_timestamp(&self.started_at_utc)?,
            ended_at_utc: self
                .ended_at_utc
                .as_deref()
                .map(parse_timestamp)
                .transpose()?,
            pending_reason: self.pending_reason,
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

impl SessionRepository for SqliteStore {
    fn create(
        &self,
        session: &Session,
        started_item: Option<(&DailyPlanItem, u64)>,
    ) -> AppResult<()> {
        session.validate()?;
        if session.revision() != 1
            || session.state() != SessionState::Focus
            || session.segments().len() != 1
            || session.segments()[0].revision() != 1
        {
            return Err(AppError::InvalidSession("initial_state"));
        }
        self.transact(|transaction| {
            if active_session_exists(transaction)? {
                return Err(AppError::ActiveSessionExists);
            }
            if historical_session_overlaps(transaction, session.started_at_utc())? {
                return Err(AppError::SessionOverlap);
            }
            validate_session_references(transaction, session)?;
            insert_session(transaction, session)?;
            insert_segment(transaction, &session.segments()[0])?;
            if let Some((item, expected_revision)) = started_item {
                update_started_item(transaction, session, item, expected_revision)?;
            }
            Ok(())
        })
    }

    fn get(&self, id: Uuid) -> AppResult<Option<Session>> {
        let connection = self.lock()?;
        load_session(&connection, "id = ?1", [id.to_string()])
    }

    fn get_active(&self) -> AppResult<Option<Session>> {
        let connection = self.lock()?;
        load_session(
            &connection,
            "ended_at_utc IS NULL AND deleted_at_utc IS NULL AND ?1 = ?1",
            ["active".to_owned()],
        )
    }

    fn save(&self, session: &Session, expected_revision: u64) -> AppResult<()> {
        session.validate()?;
        if session.revision()
            != expected_revision
                .checked_add(1)
                .ok_or(AppError::RevisionConflict)?
        {
            return Err(AppError::RevisionConflict);
        }
        self.transact(|transaction| {
            let changed = transaction
                .execute(
                    "UPDATE sessions SET
                        state = ?1,
                        ended_at_utc = ?2,
                        timer_config_json = ?3,
                        recovery_checkpoint_utc = ?4,
                        updated_at_utc = ?5,
                        revision = ?6
                     WHERE id = ?7 AND revision = ?8 AND deleted_at_utc IS NULL",
                    params![
                        session.state().as_str(),
                        format_timestamp(session.ended_at_utc()),
                        session.timer_config_json(),
                        session.recovery_checkpoint_utc().to_rfc3339(),
                        session.updated_at_utc().to_rfc3339(),
                        revision_to_sql(session.revision())?,
                        session.id().to_string(),
                        revision_to_sql(expected_revision)?,
                    ],
                )
                .map_err(|_| AppError::Database)?;
            if changed == 0 {
                return Err(if session_exists(transaction, session.id())? {
                    AppError::RevisionConflict
                } else {
                    AppError::SessionNotFound
                });
            }
            for segment in session.segments() {
                save_segment(transaction, segment)?;
            }
            transaction
                .execute(
                    "INSERT INTO audit_revisions (
                        id, entity_kind, entity_id, previous_revision, new_revision,
                        change_summary_json, changed_at_utc, source_device_id
                     ) VALUES (?1, 'session', ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        Uuid::now_v7().to_string(),
                        session.id().to_string(),
                        revision_to_sql(expected_revision)?,
                        revision_to_sql(session.revision())?,
                        r#"{"operation":"session_transition"}"#,
                        session.updated_at_utc().to_rfc3339(),
                        session.source_device_id().to_string(),
                    ],
                )
                .map_err(|_| AppError::Database)?;
            Ok(())
        })
    }
}

fn load_session(
    connection: &Connection,
    predicate: &str,
    parameters: [String; 1],
) -> AppResult<Option<Session>> {
    let query = format!(
        "SELECT id, subject_id, task_id, daily_plan_item_id, timer_mode, state,
                started_at_utc, ended_at_utc, time_zone_id, timer_config_json,
                recovery_checkpoint_utc, created_at_utc, updated_at_utc,
                source_device_id, revision, deleted_at_utc
         FROM sessions WHERE {predicate} LIMIT 1"
    );
    let record = connection
        .query_row(&query, parameters, read_session_record)
        .optional()
        .map_err(|_| AppError::Database)?;
    let Some(record) = record else {
        return Ok(None);
    };
    let session_id = parse_uuid(&record.id)?;
    let segments = load_segments(connection, session_id)?;
    record.into_session(segments).map(Some)
}

fn load_segments(connection: &Connection, session_id: Uuid) -> AppResult<Vec<SessionSegment>> {
    let mut statement = connection
        .prepare(
            "SELECT id, session_id, kind, started_at_utc, ended_at_utc, pending_reason,
                    created_at_utc, updated_at_utc, source_device_id, revision, deleted_at_utc
             FROM session_segments
             WHERE session_id = ?1 AND deleted_at_utc IS NULL
             ORDER BY started_at_utc ASC, id ASC",
        )
        .map_err(|_| AppError::Database)?;
    let records = statement
        .query_map([session_id.to_string()], read_segment_record)
        .map_err(|_| AppError::Database)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::Database)?;
    records
        .into_iter()
        .map(SegmentRecord::into_segment)
        .collect()
}

fn validate_session_references(transaction: &Transaction<'_>, session: &Session) -> AppResult<()> {
    let subject = transaction
        .query_row(
            "SELECT archived_at_utc FROM subjects WHERE id = ?1 AND deleted_at_utc IS NULL",
            [session.subject_id().to_string()],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|_| AppError::Database)?;
    match subject {
        None => return Err(AppError::SubjectNotFound),
        Some(Some(_)) => return Err(AppError::SubjectArchived),
        Some(None) => {}
    }
    if let Some(task_id) = session.task_id() {
        let task = transaction
            .query_row(
                "SELECT subject_id, status FROM tasks WHERE id = ?1 AND deleted_at_utc IS NULL",
                [task_id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|_| AppError::Database)?;
        match task {
            None => return Err(AppError::TaskNotFound),
            Some((_, status)) if status == "archived" => return Err(AppError::TaskArchived),
            Some((subject_id, _)) if subject_id != session.subject_id().to_string() => {
                return Err(AppError::SessionReferenceMismatch);
            }
            Some(_) => {}
        }
    }
    if let Some(item_id) = session.daily_plan_item_id() {
        let item = transaction
            .query_row(
                "SELECT subject_id, task_id FROM daily_plan_items
                 WHERE id = ?1 AND deleted_at_utc IS NULL",
                [item_id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()
            .map_err(|_| AppError::Database)?;
        match item {
            None => return Err(AppError::DailyPlanItemNotFound),
            Some((subject_id, task_id))
                if subject_id != session.subject_id().to_string()
                    || task_id.as_deref().is_some_and(|id| {
                        Some(id) != session.task_id().map(|id| id.to_string()).as_deref()
                    }) =>
            {
                return Err(AppError::SessionReferenceMismatch);
            }
            Some(_) => {}
        }
    }
    Ok(())
}

fn insert_session(transaction: &Transaction<'_>, session: &Session) -> AppResult<()> {
    transaction
        .execute(
            "INSERT INTO sessions (
                id, subject_id, task_id, daily_plan_item_id, timer_mode, state,
                started_at_utc, ended_at_utc, time_zone_id, timer_config_json,
                recovery_checkpoint_utc, created_at_utc, updated_at_utc,
                source_device_id, revision, deleted_at_utc
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                session.id().to_string(),
                session.subject_id().to_string(),
                session.task_id().map(|id| id.to_string()),
                session.daily_plan_item_id().map(|id| id.to_string()),
                session.timer_mode().as_str(),
                session.state().as_str(),
                session.started_at_utc().to_rfc3339(),
                format_timestamp(session.ended_at_utc()),
                session.time_zone().name(),
                session.timer_config_json(),
                session.recovery_checkpoint_utc().to_rfc3339(),
                session.created_at_utc().to_rfc3339(),
                session.updated_at_utc().to_rfc3339(),
                session.source_device_id().to_string(),
                revision_to_sql(session.revision())?,
                format_timestamp(session.deleted_at_utc()),
            ],
        )
        .map_err(|_| AppError::Database)?;
    Ok(())
}

fn insert_segment(transaction: &Transaction<'_>, segment: &SessionSegment) -> AppResult<()> {
    transaction
        .execute(
            "INSERT INTO session_segments (
                id, session_id, kind, started_at_utc, ended_at_utc, pending_reason,
                created_at_utc, updated_at_utc, source_device_id, revision, deleted_at_utc
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                segment.id().to_string(),
                segment.session_id().to_string(),
                segment.kind().as_str(),
                segment.started_at_utc().to_rfc3339(),
                format_timestamp(segment.ended_at_utc()),
                segment.pending_reason(),
                segment.created_at_utc().to_rfc3339(),
                segment.updated_at_utc().to_rfc3339(),
                segment.source_device_id().to_string(),
                revision_to_sql(segment.revision())?,
                format_timestamp(segment.deleted_at_utc()),
            ],
        )
        .map_err(|_| AppError::Database)?;
    Ok(())
}

fn save_segment(transaction: &Transaction<'_>, segment: &SessionSegment) -> AppResult<()> {
    let stored = transaction
        .query_row(
            "SELECT session_id, kind, started_at_utc, ended_at_utc, pending_reason,
                    updated_at_utc, source_device_id, revision, deleted_at_utc
             FROM session_segments WHERE id = ?1",
            [segment.id().to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, Option<String>>(8)?,
                ))
            },
        )
        .optional()
        .map_err(|_| AppError::Database)?;
    let Some((
        session_id,
        kind,
        started_at,
        ended_at,
        pending_reason,
        updated_at,
        source_device_id,
        stored_revision,
        deleted_at,
    )) = stored
    else {
        if segment.revision() != 1 {
            return Err(AppError::RevisionConflict);
        }
        return insert_segment(transaction, segment);
    };
    if session_id != segment.session_id().to_string()
        || kind != segment.kind().as_str()
        || parse_timestamp(&started_at)? != segment.started_at_utc()
        || source_device_id != segment.source_device_id().to_string()
    {
        return Err(AppError::InvalidSession("segment_identity"));
    }
    let stored_revision = u64::try_from(stored_revision).map_err(|_| AppError::Serialization)?;
    if stored_revision == segment.revision() {
        return if ended_at == format_timestamp(segment.ended_at_utc())
            && pending_reason.as_deref() == segment.pending_reason()
            && parse_timestamp(&updated_at)? == segment.updated_at_utc()
            && deleted_at == format_timestamp(segment.deleted_at_utc())
        {
            Ok(())
        } else {
            Err(AppError::RevisionConflict)
        };
    }
    if stored_revision.checked_add(1) != Some(segment.revision()) {
        return Err(AppError::RevisionConflict);
    }
    let changed = transaction
        .execute(
            "UPDATE session_segments SET
                ended_at_utc = ?1,
                pending_reason = ?2,
                updated_at_utc = ?3,
                revision = ?4,
                deleted_at_utc = ?5
             WHERE id = ?6 AND revision = ?7",
            params![
                format_timestamp(segment.ended_at_utc()),
                segment.pending_reason(),
                segment.updated_at_utc().to_rfc3339(),
                revision_to_sql(segment.revision())?,
                format_timestamp(segment.deleted_at_utc()),
                segment.id().to_string(),
                revision_to_sql(stored_revision)?,
            ],
        )
        .map_err(|_| AppError::Database)?;
    if changed == 1 {
        Ok(())
    } else {
        Err(AppError::RevisionConflict)
    }
}

fn update_started_item(
    transaction: &Transaction<'_>,
    session: &Session,
    item: &DailyPlanItem,
    expected_revision: u64,
) -> AppResult<()> {
    if Some(item.id()) != session.daily_plan_item_id()
        || item.status() != DailyPlanItemStatus::InProgress
        || item.revision()
            != expected_revision
                .checked_add(1)
                .ok_or(AppError::RevisionConflict)?
    {
        return Err(AppError::SessionReferenceMismatch);
    }
    let changed = transaction
        .execute(
            "UPDATE daily_plan_items SET
                status = 'in_progress', updated_at_utc = ?1, revision = ?2
             WHERE id = ?3 AND subject_id = ?4 AND task_id IS ?5
               AND revision = ?6 AND deleted_at_utc IS NULL",
            params![
                item.updated_at_utc().to_rfc3339(),
                revision_to_sql(item.revision())?,
                item.id().to_string(),
                item.subject_id().to_string(),
                item.task_id().map(|id| id.to_string()),
                revision_to_sql(expected_revision)?,
            ],
        )
        .map_err(|_| AppError::Database)?;
    if changed != 1 {
        return Err(AppError::RevisionConflict);
    }
    let plan_changed = transaction
        .execute(
            "UPDATE daily_plans SET updated_at_utc = ?1, revision = revision + 1
             WHERE id = ?2 AND deleted_at_utc IS NULL",
            params![
                item.updated_at_utc().to_rfc3339(),
                item.daily_plan_id().to_string()
            ],
        )
        .map_err(|_| AppError::Database)?;
    if plan_changed != 1 {
        return Err(AppError::DailyPlanNotFound);
    }
    transaction
        .execute(
            "INSERT INTO audit_revisions (
                id, entity_kind, entity_id, previous_revision, new_revision,
                change_summary_json, changed_at_utc, source_device_id
             ) VALUES (?1, 'daily_plan_item', ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                Uuid::now_v7().to_string(),
                item.id().to_string(),
                revision_to_sql(expected_revision)?,
                revision_to_sql(item.revision())?,
                r#"{"operation":"session_started"}"#,
                item.updated_at_utc().to_rfc3339(),
                item.source_device_id().to_string(),
            ],
        )
        .map_err(|_| AppError::Database)?;
    Ok(())
}

fn active_session_exists(transaction: &Transaction<'_>) -> AppResult<bool> {
    transaction
        .query_row(
            "SELECT 1 FROM sessions WHERE ended_at_utc IS NULL AND deleted_at_utc IS NULL LIMIT 1",
            [],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(|_| AppError::Database)
}

fn historical_session_overlaps(
    transaction: &Transaction<'_>,
    started_at: chrono::DateTime<chrono::Utc>,
) -> AppResult<bool> {
    transaction
        .query_row(
            "SELECT 1 FROM sessions
             WHERE deleted_at_utc IS NULL
               AND ended_at_utc IS NOT NULL
               AND ended_at_utc > ?1
             LIMIT 1",
            [started_at.to_rfc3339()],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(|_| AppError::Database)
}

fn session_exists(transaction: &Transaction<'_>, id: Uuid) -> AppResult<bool> {
    transaction
        .query_row(
            "SELECT 1 FROM sessions WHERE id = ?1 AND deleted_at_utc IS NULL",
            [id.to_string()],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(|_| AppError::Database)
}

fn read_session_record(row: &Row<'_>) -> rusqlite::Result<SessionRecord> {
    Ok(SessionRecord {
        id: row.get(0)?,
        subject_id: row.get(1)?,
        task_id: row.get(2)?,
        daily_plan_item_id: row.get(3)?,
        timer_mode: row.get(4)?,
        state: row.get(5)?,
        started_at_utc: row.get(6)?,
        ended_at_utc: row.get(7)?,
        time_zone_id: row.get(8)?,
        timer_config_json: row.get(9)?,
        recovery_checkpoint_utc: row.get(10)?,
        created_at_utc: row.get(11)?,
        updated_at_utc: row.get(12)?,
        source_device_id: row.get(13)?,
        revision: row.get(14)?,
        deleted_at_utc: row.get(15)?,
    })
}

fn read_segment_record(row: &Row<'_>) -> rusqlite::Result<SegmentRecord> {
    Ok(SegmentRecord {
        id: row.get(0)?,
        session_id: row.get(1)?,
        kind: row.get(2)?,
        started_at_utc: row.get(3)?,
        ended_at_utc: row.get(4)?,
        pending_reason: row.get(5)?,
        created_at_utc: row.get(6)?,
        updated_at_utc: row.get(7)?,
        source_device_id: row.get(8)?,
        revision: row.get(9)?,
        deleted_at_utc: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use chrono_tz::Asia::Taipei;
    use uuid::Uuid;

    use super::SqliteStore;
    use crate::{
        domain::{
            daily_plan::{DailyPlan, DailyPlanItem, DailyPlanItemIds, DailyPlanItemStatus},
            session::{Session, SessionIds, SessionState, TimerMode},
            study_task::StudyTask,
            subject::Subject,
        },
        error::AppError,
        ports::repository::{
            DailyPlanRepository, SessionRepository, SubjectRepository, TaskRepository,
        },
    };

    fn fixture() -> (SqliteStore, Uuid, Subject) {
        let store = SqliteStore::in_memory().unwrap();
        let device_id = Uuid::now_v7();
        let now = Utc.with_ymd_and_hms(2026, 7, 23, 8, 0, 0).unwrap();
        {
            let connection = store.lock().unwrap();
            connection
                .execute(
                    "INSERT INTO devices (
                        id, display_name, created_at_utc, updated_at_utc, revision
                     ) VALUES (?1, '測試裝置', ?2, ?2, 1)",
                    (device_id.to_string(), now.to_rfc3339()),
                )
                .unwrap();
        }
        let subject = Subject::create(Uuid::now_v7(), "數學", 0, now, device_id).unwrap();
        SubjectRepository::insert(&store, &subject).unwrap();
        (store, device_id, subject)
    }

    fn new_session(device_id: Uuid, subject: &Subject) -> Session {
        Session::start(
            SessionIds {
                id: Uuid::now_v7(),
                first_segment_id: Uuid::now_v7(),
                subject_id: subject.id(),
                task_id: None,
                daily_plan_item_id: None,
            },
            TimerMode::Stopwatch,
            Taipei,
            "{}",
            subject.created_at_utc(),
            device_id,
        )
        .unwrap()
    }

    #[test]
    fn 原子建立工作階段與第一個區段並拒絕第二個活動項目() {
        let (store, device_id, subject) = fixture();
        let first = new_session(device_id, &subject);
        SessionRepository::create(&store, &first, None).unwrap();
        let second = new_session(device_id, &subject);
        assert!(matches!(
            SessionRepository::create(&store, &second, None),
            Err(AppError::ActiveSessionExists)
        ));
        let active = SessionRepository::get_active(&store).unwrap().unwrap();
        assert_eq!(active.id(), first.id());
        assert_eq!(active.segments().len(), 1);
    }

    #[test]
    fn 狀態轉移與區段以同一交易保存() {
        let (store, device_id, subject) = fixture();
        let mut session = new_session(device_id, &subject);
        SessionRepository::create(&store, &session, None).unwrap();
        let revision = session.revision();
        session
            .pause(
                Uuid::now_v7(),
                session.started_at_utc() + Duration::minutes(10),
            )
            .unwrap();
        SessionRepository::save(&store, &session, revision).unwrap();
        let loaded = SessionRepository::get(&store, session.id())
            .unwrap()
            .unwrap();
        assert_eq!(loaded.state(), SessionState::Paused);
        assert_eq!(loaded.segments().len(), 2);
        assert_eq!(
            loaded.segments()[0].ended_at_utc(),
            Some(loaded.segments()[1].started_at_utc())
        );

        let stale = loaded.clone();
        let mut ended = loaded;
        let revision = ended.revision();
        ended
            .end(ended.started_at_utc() + Duration::minutes(20))
            .unwrap();
        SessionRepository::save(&store, &ended, revision).unwrap();
        assert!(matches!(
            SessionRepository::save(&store, &stale, stale.revision() - 1),
            Err(AppError::RevisionConflict)
        ));
        assert!(SessionRepository::get_active(&store).unwrap().is_none());

        let overlapping = new_session(device_id, &subject);
        assert!(matches!(
            SessionRepository::create(&store, &overlapping, None),
            Err(AppError::SessionOverlap)
        ));
    }

    #[test]
    fn 活動衝突會回滾計畫項目的進行中狀態() {
        let (store, device_id, subject) = fixture();
        let now = subject.created_at_utc();
        let task =
            StudyTask::create(Uuid::now_v7(), subject.id(), "練習第一章", now, device_id).unwrap();
        TaskRepository::insert(&store, &task).unwrap();
        let plan = DailyPlan::create(Uuid::now_v7(), now.date_naive(), Taipei, now, device_id);
        let plan = DailyPlanRepository::get_or_create(&store, &plan).unwrap();

        let create_item = || {
            DailyPlanItem::create(
                DailyPlanItemIds {
                    id: Uuid::now_v7(),
                    daily_plan_id: plan.id(),
                    subject_id: subject.id(),
                    task_id: Some(task.id()),
                },
                "今日數學",
                0,
                now,
                device_id,
            )
            .unwrap()
        };
        let first_item = create_item();
        let second_item = create_item();
        DailyPlanRepository::insert_item(&store, &first_item).unwrap();
        DailyPlanRepository::insert_item(&store, &second_item).unwrap();

        let create_session = |item: &DailyPlanItem| {
            Session::start(
                SessionIds {
                    id: Uuid::now_v7(),
                    first_segment_id: Uuid::now_v7(),
                    subject_id: subject.id(),
                    task_id: Some(task.id()),
                    daily_plan_item_id: Some(item.id()),
                },
                TimerMode::Stopwatch,
                Taipei,
                "{}",
                now,
                device_id,
            )
            .unwrap()
        };
        let mut started_first_item = first_item.clone();
        started_first_item
            .set_status(DailyPlanItemStatus::InProgress, now)
            .unwrap();
        SessionRepository::create(
            &store,
            &create_session(&first_item),
            Some((&started_first_item, first_item.revision())),
        )
        .unwrap();

        let mut started_second_item = second_item.clone();
        started_second_item
            .set_status(DailyPlanItemStatus::InProgress, now)
            .unwrap();
        assert!(matches!(
            SessionRepository::create(
                &store,
                &create_session(&second_item),
                Some((&started_second_item, second_item.revision()))
            ),
            Err(AppError::ActiveSessionExists)
        ));
        assert_eq!(
            DailyPlanRepository::get_item(&store, second_item.id())
                .unwrap()
                .unwrap()
                .status(),
            DailyPlanItemStatus::Planned
        );
    }
}
