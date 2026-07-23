use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use rusqlite::{OptionalExtension, Row, Transaction, params};
use uuid::Uuid;

use super::{SqliteStore, format_timestamp, parse_timestamp, parse_uuid, revision_to_sql};
use crate::{
    domain::daily_plan::{
        DailyPlan, DailyPlanItem, DailyPlanItemSnapshot, DailyPlanItemStatus, DailyPlanSnapshot,
    },
    error::{AppError, AppResult},
    ports::repository::DailyPlanRepository,
};

struct DailyPlanRecord {
    id: String,
    local_date: String,
    time_zone_id: String,
    created_at_utc: String,
    updated_at_utc: String,
    source_device_id: String,
    revision: i64,
    deleted_at_utc: Option<String>,
}

impl DailyPlanRecord {
    fn into_plan(self) -> AppResult<DailyPlan> {
        DailyPlan::restore(DailyPlanSnapshot {
            id: parse_uuid(&self.id)?,
            local_date: parse_date(&self.local_date)?,
            time_zone: self
                .time_zone_id
                .parse::<Tz>()
                .map_err(|_| AppError::Serialization)?,
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

struct DailyPlanItemRecord {
    id: String,
    daily_plan_id: String,
    subject_id: String,
    task_id: Option<String>,
    title: String,
    status: String,
    sort_order: i64,
    created_at_utc: String,
    updated_at_utc: String,
    source_device_id: String,
    revision: i64,
    deleted_at_utc: Option<String>,
}

impl DailyPlanItemRecord {
    fn into_item(self) -> AppResult<DailyPlanItem> {
        DailyPlanItem::restore(DailyPlanItemSnapshot {
            id: parse_uuid(&self.id)?,
            daily_plan_id: parse_uuid(&self.daily_plan_id)?,
            subject_id: parse_uuid(&self.subject_id)?,
            task_id: self.task_id.as_deref().map(parse_uuid).transpose()?,
            title: self.title,
            status: DailyPlanItemStatus::from_persisted(&self.status)?,
            sort_order: self.sort_order,
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

impl DailyPlanRepository for SqliteStore {
    fn get_or_create(&self, candidate: &DailyPlan) -> AppResult<DailyPlan> {
        self.transact(|transaction| {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO daily_plans (
                        id, local_date, time_zone_id, created_at_utc, updated_at_utc,
                        source_device_id, revision, deleted_at_utc
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        candidate.id().to_string(),
                        candidate.local_date().to_string(),
                        candidate.time_zone().name(),
                        candidate.created_at_utc().to_rfc3339(),
                        candidate.updated_at_utc().to_rfc3339(),
                        candidate.source_device_id().to_string(),
                        revision_to_sql(candidate.revision())?,
                        format_timestamp(candidate.deleted_at_utc()),
                    ],
                )
                .map_err(|_| AppError::Database)?;
            let record = transaction
                .query_row(
                    "SELECT id, local_date, time_zone_id, created_at_utc, updated_at_utc,
                            source_device_id, revision, deleted_at_utc
                     FROM daily_plans
                     WHERE source_device_id = ?1 AND local_date = ?2 AND deleted_at_utc IS NULL",
                    params![
                        candidate.source_device_id().to_string(),
                        candidate.local_date().to_string()
                    ],
                    read_plan_record,
                )
                .map_err(|_| AppError::Database)?;
            record.into_plan()
        })
    }

    fn get_by_date(
        &self,
        source_device_id: Uuid,
        local_date: NaiveDate,
    ) -> AppResult<Option<DailyPlan>> {
        let record = {
            let connection = self.lock()?;
            connection
                .query_row(
                    "SELECT id, local_date, time_zone_id, created_at_utc, updated_at_utc,
                            source_device_id, revision, deleted_at_utc
                     FROM daily_plans
                     WHERE source_device_id = ?1 AND local_date = ?2 AND deleted_at_utc IS NULL",
                    params![source_device_id.to_string(), local_date.to_string()],
                    read_plan_record,
                )
                .optional()
                .map_err(|_| AppError::Database)?
        };
        record.map(DailyPlanRecord::into_plan).transpose()
    }

    fn insert_item(&self, item: &DailyPlanItem) -> AppResult<()> {
        self.transact(|transaction| {
            let task_id = item.task_id().map(|id| id.to_string());
            let inserted = transaction
                .execute(
                    "INSERT INTO daily_plan_items (
                        id, daily_plan_id, subject_id, task_id, title, status, sort_order,
                        created_at_utc, updated_at_utc, source_device_id, revision, deleted_at_utc
                     )
                     SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12
                     WHERE EXISTS (
                         SELECT 1 FROM daily_plans
                         WHERE id = ?2 AND deleted_at_utc IS NULL
                     )
                     AND EXISTS (
                         SELECT 1 FROM subjects
                         WHERE id = ?3 AND deleted_at_utc IS NULL
                     )
                     AND (
                         ?4 IS NULL OR EXISTS (
                             SELECT 1 FROM tasks
                             WHERE id = ?4 AND subject_id = ?3 AND deleted_at_utc IS NULL
                         )
                     )",
                    params![
                        item.id().to_string(),
                        item.daily_plan_id().to_string(),
                        item.subject_id().to_string(),
                        task_id.as_deref(),
                        item.title(),
                        item.status().as_str(),
                        item.sort_order(),
                        item.created_at_utc().to_rfc3339(),
                        item.updated_at_utc().to_rfc3339(),
                        item.source_device_id().to_string(),
                        revision_to_sql(item.revision())?,
                        format_timestamp(item.deleted_at_utc()),
                    ],
                )
                .map_err(|_| AppError::Database)?;
            if inserted == 0 {
                validate_references(transaction, item)?;
                return Err(AppError::Database);
            }
            touch_plan(transaction, item.daily_plan_id(), item.updated_at_utc())?;
            Ok(())
        })
    }

    fn get_item(&self, id: Uuid) -> AppResult<Option<DailyPlanItem>> {
        let record = {
            let connection = self.lock()?;
            connection
                .query_row(ITEM_SELECT_BY_ID, [id.to_string()], read_item_record)
                .optional()
                .map_err(|_| AppError::Database)?
        };
        record.map(DailyPlanItemRecord::into_item).transpose()
    }

    fn list_items(&self, daily_plan_id: Uuid) -> AppResult<Vec<DailyPlanItem>> {
        let records = {
            let connection = self.lock()?;
            let mut statement = connection
                .prepare(
                    "SELECT id, daily_plan_id, subject_id, task_id, title, status, sort_order,
                            created_at_utc, updated_at_utc, source_device_id, revision, deleted_at_utc
                     FROM daily_plan_items
                     WHERE daily_plan_id = ?1 AND deleted_at_utc IS NULL
                     ORDER BY sort_order ASC, title COLLATE NOCASE ASC, id ASC",
                )
                .map_err(|_| AppError::Database)?;
            statement
                .query_map([daily_plan_id.to_string()], read_item_record)
                .map_err(|_| AppError::Database)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| AppError::Database)?
        };
        records
            .into_iter()
            .map(DailyPlanItemRecord::into_item)
            .collect()
    }

    fn update_item(&self, item: &DailyPlanItem, expected_revision: u64) -> AppResult<()> {
        if item.revision()
            != expected_revision
                .checked_add(1)
                .ok_or(AppError::RevisionConflict)?
        {
            return Err(AppError::RevisionConflict);
        }
        self.transact(|transaction| {
            let task_id = item.task_id().map(|id| id.to_string());
            let changed = transaction
                .execute(
                    "UPDATE daily_plan_items SET
                        title = ?1,
                        status = ?2,
                        sort_order = ?3,
                        updated_at_utc = ?4,
                        revision = ?5
                     WHERE id = ?6
                       AND daily_plan_id = ?7
                       AND subject_id = ?8
                       AND task_id IS ?9
                       AND revision = ?10
                       AND deleted_at_utc IS NULL",
                    params![
                        item.title(),
                        item.status().as_str(),
                        item.sort_order(),
                        item.updated_at_utc().to_rfc3339(),
                        revision_to_sql(item.revision())?,
                        item.id().to_string(),
                        item.daily_plan_id().to_string(),
                        item.subject_id().to_string(),
                        task_id.as_deref(),
                        revision_to_sql(expected_revision)?,
                    ],
                )
                .map_err(|_| AppError::Database)?;
            if changed == 0 {
                let identity = transaction
                    .query_row(
                        "SELECT daily_plan_id, subject_id, task_id
                         FROM daily_plan_items
                         WHERE id = ?1 AND deleted_at_utc IS NULL",
                        [item.id().to_string()],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, Option<String>>(2)?,
                            ))
                        },
                    )
                    .optional()
                    .map_err(|_| AppError::Database)?;
                return Err(match identity {
                    None => AppError::DailyPlanItemNotFound,
                    Some((plan_id, subject_id, stored_task_id))
                        if plan_id != item.daily_plan_id().to_string()
                            || subject_id != item.subject_id().to_string()
                            || stored_task_id != task_id =>
                    {
                        AppError::InvalidDailyPlan("item_identity")
                    }
                    Some(_) => AppError::RevisionConflict,
                });
            }
            touch_plan(transaction, item.daily_plan_id(), item.updated_at_utc())?;
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
                        r#"{"operation":"daily_plan_item_update"}"#,
                        item.updated_at_utc().to_rfc3339(),
                        item.source_device_id().to_string(),
                    ],
                )
                .map_err(|_| AppError::Database)?;
            Ok(())
        })
    }
}

const ITEM_SELECT_BY_ID: &str =
    "SELECT id, daily_plan_id, subject_id, task_id, title, status, sort_order,
            created_at_utc, updated_at_utc, source_device_id, revision, deleted_at_utc
     FROM daily_plan_items
     WHERE id = ?1 AND deleted_at_utc IS NULL";

fn validate_references(transaction: &Transaction<'_>, item: &DailyPlanItem) -> AppResult<()> {
    let plan_exists = exists(
        transaction,
        "SELECT 1 FROM daily_plans WHERE id = ?1 AND deleted_at_utc IS NULL",
        item.daily_plan_id(),
    )?;
    if !plan_exists {
        return Err(AppError::DailyPlanNotFound);
    }
    let subject_exists = exists(
        transaction,
        "SELECT 1 FROM subjects WHERE id = ?1 AND deleted_at_utc IS NULL",
        item.subject_id(),
    )?;
    if !subject_exists {
        return Err(AppError::SubjectNotFound);
    }
    if let Some(task_id) = item.task_id() {
        let task_subject = transaction
            .query_row(
                "SELECT subject_id FROM tasks WHERE id = ?1 AND deleted_at_utc IS NULL",
                [task_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| AppError::Database)?;
        match task_subject {
            None => return Err(AppError::TaskNotFound),
            Some(subject_id) if subject_id != item.subject_id().to_string() => {
                return Err(AppError::DailyPlanReferenceMismatch);
            }
            Some(_) => {}
        }
    }
    Ok(())
}

fn exists(transaction: &Transaction<'_>, query: &str, id: Uuid) -> AppResult<bool> {
    transaction
        .query_row(query, [id.to_string()], |_| Ok(()))
        .optional()
        .map(|value| value.is_some())
        .map_err(|_| AppError::Database)
}

fn touch_plan(
    transaction: &Transaction<'_>,
    plan_id: Uuid,
    changed_at: DateTime<Utc>,
) -> AppResult<()> {
    let changed = transaction
        .execute(
            "UPDATE daily_plans
             SET updated_at_utc = ?1, revision = revision + 1
             WHERE id = ?2 AND deleted_at_utc IS NULL",
            params![changed_at.to_rfc3339(), plan_id.to_string()],
        )
        .map_err(|_| AppError::Database)?;
    if changed == 1 {
        Ok(())
    } else {
        Err(AppError::DailyPlanNotFound)
    }
}

fn read_plan_record(row: &Row<'_>) -> rusqlite::Result<DailyPlanRecord> {
    Ok(DailyPlanRecord {
        id: row.get(0)?,
        local_date: row.get(1)?,
        time_zone_id: row.get(2)?,
        created_at_utc: row.get(3)?,
        updated_at_utc: row.get(4)?,
        source_device_id: row.get(5)?,
        revision: row.get(6)?,
        deleted_at_utc: row.get(7)?,
    })
}

fn read_item_record(row: &Row<'_>) -> rusqlite::Result<DailyPlanItemRecord> {
    Ok(DailyPlanItemRecord {
        id: row.get(0)?,
        daily_plan_id: row.get(1)?,
        subject_id: row.get(2)?,
        task_id: row.get(3)?,
        title: row.get(4)?,
        status: row.get(5)?,
        sort_order: row.get(6)?,
        created_at_utc: row.get(7)?,
        updated_at_utc: row.get(8)?,
        source_device_id: row.get(9)?,
        revision: row.get(10)?,
        deleted_at_utc: row.get(11)?,
    })
}

fn parse_date(value: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| AppError::Serialization)
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, NaiveDate, TimeZone, Utc};
    use chrono_tz::Asia::Taipei;
    use uuid::Uuid;

    use super::SqliteStore;
    use crate::{
        domain::{
            daily_plan::{DailyPlan, DailyPlanItem, DailyPlanItemIds, DailyPlanItemStatus},
            study_task::{StudyTask, TaskStatus},
            subject::Subject,
        },
        error::AppError,
        ports::repository::{DailyPlanRepository, SubjectRepository, TaskRepository},
    };

    fn fixture() -> (SqliteStore, Uuid, Subject, StudyTask, DailyPlan) {
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
        let task =
            StudyTask::create(Uuid::now_v7(), subject.id(), "練習第一章", now, device_id).unwrap();
        TaskRepository::insert(&store, &task).unwrap();
        let plan = DailyPlan::create(
            Uuid::now_v7(),
            NaiveDate::from_ymd_opt(2026, 7, 23).unwrap(),
            Taipei,
            now,
            device_id,
        );
        let plan = DailyPlanRepository::get_or_create(&store, &plan).unwrap();
        (store, device_id, subject, task, plan)
    }

    fn item(
        device_id: Uuid,
        subject: &Subject,
        task_id: Option<Uuid>,
        plan: &DailyPlan,
        sort_order: i64,
    ) -> DailyPlanItem {
        DailyPlanItem::create(
            DailyPlanItemIds {
                id: Uuid::now_v7(),
                daily_plan_id: plan.id(),
                subject_id: subject.id(),
                task_id,
            },
            "今日數學",
            sort_order,
            plan.created_at_utc(),
            device_id,
        )
        .unwrap()
    }

    #[test]
    fn 同日期同裝置只建立一份計畫且項目依序保存() {
        let (store, device_id, subject, task, plan) = fixture();
        let duplicate = DailyPlan::create(
            Uuid::now_v7(),
            plan.local_date(),
            plan.time_zone(),
            plan.created_at_utc(),
            device_id,
        );
        assert_eq!(
            DailyPlanRepository::get_or_create(&store, &duplicate)
                .unwrap()
                .id(),
            plan.id()
        );
        let later = item(device_id, &subject, None, &plan, 10);
        let earlier = item(device_id, &subject, Some(task.id()), &plan, 0);
        DailyPlanRepository::insert_item(&store, &later).unwrap();
        DailyPlanRepository::insert_item(&store, &earlier).unwrap();
        let items = DailyPlanRepository::list_items(&store, plan.id()).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id(), earlier.id());
    }

    #[test]
    fn 資料層原子拒絕任務與科目不一致() {
        let (store, device_id, _, task, plan) = fixture();
        let other_subject =
            Subject::create(Uuid::now_v7(), "英文", 1, plan.created_at_utc(), device_id).unwrap();
        SubjectRepository::insert(&store, &other_subject).unwrap();
        let invalid = item(device_id, &other_subject, Some(task.id()), &plan, 0);
        assert!(matches!(
            DailyPlanRepository::insert_item(&store, &invalid),
            Err(AppError::DailyPlanReferenceMismatch)
        ));
        assert!(
            DailyPlanRepository::get_item(&store, invalid.id())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn 項目狀態獨立更新並使用樂觀版本() {
        let (store, device_id, subject, task, plan) = fixture();
        let item = item(device_id, &subject, Some(task.id()), &plan, 0);
        DailyPlanRepository::insert_item(&store, &item).unwrap();
        let mut changed = item.clone();
        changed
            .set_status(
                DailyPlanItemStatus::InProgress,
                item.created_at_utc() + Duration::minutes(1),
            )
            .unwrap();
        let mut stale = item;
        stale
            .set_status(
                DailyPlanItemStatus::Deferred,
                stale.created_at_utc() + Duration::minutes(1),
            )
            .unwrap();
        DailyPlanRepository::update_item(&store, &changed, 1).unwrap();
        assert!(matches!(
            DailyPlanRepository::update_item(&store, &stale, 1),
            Err(AppError::RevisionConflict)
        ));
        assert_eq!(
            TaskRepository::get(&store, task.id())
                .unwrap()
                .unwrap()
                .status(),
            TaskStatus::Open
        );
        assert_eq!(
            DailyPlanRepository::get_item(&store, changed.id())
                .unwrap()
                .unwrap()
                .status(),
            DailyPlanItemStatus::InProgress
        );
        let refreshed_plan = DailyPlanRepository::get_by_date(&store, device_id, plan.local_date())
            .unwrap()
            .unwrap();
        assert_eq!(refreshed_plan.revision(), 3);
    }
}
