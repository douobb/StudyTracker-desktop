use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyPlan {
    id: Uuid,
    local_date: NaiveDate,
    time_zone: Tz,
    created_at_utc: DateTime<Utc>,
    updated_at_utc: DateTime<Utc>,
    source_device_id: Uuid,
    revision: u64,
    deleted_at_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyPlanSnapshot {
    pub id: Uuid,
    pub local_date: NaiveDate,
    pub time_zone: Tz,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    pub source_device_id: Uuid,
    pub revision: u64,
    pub deleted_at_utc: Option<DateTime<Utc>>,
}

impl DailyPlan {
    pub fn create(
        id: Uuid,
        local_date: NaiveDate,
        time_zone: Tz,
        now: DateTime<Utc>,
        source_device_id: Uuid,
    ) -> Self {
        Self {
            id,
            local_date,
            time_zone,
            created_at_utc: now,
            updated_at_utc: now,
            source_device_id,
            revision: 1,
            deleted_at_utc: None,
        }
    }

    pub fn restore(snapshot: DailyPlanSnapshot) -> AppResult<Self> {
        if snapshot.revision == 0
            || snapshot.updated_at_utc < snapshot.created_at_utc
            || snapshot
                .deleted_at_utc
                .is_some_and(|deleted_at| deleted_at < snapshot.created_at_utc)
        {
            return Err(AppError::InvalidDailyPlan("metadata"));
        }
        Ok(Self {
            id: snapshot.id,
            local_date: snapshot.local_date,
            time_zone: snapshot.time_zone,
            created_at_utc: snapshot.created_at_utc,
            updated_at_utc: snapshot.updated_at_utc,
            source_device_id: snapshot.source_device_id,
            revision: snapshot.revision,
            deleted_at_utc: snapshot.deleted_at_utc,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn local_date(&self) -> NaiveDate {
        self.local_date
    }

    pub fn time_zone(&self) -> Tz {
        self.time_zone
    }

    pub fn created_at_utc(&self) -> DateTime<Utc> {
        self.created_at_utc
    }

    pub fn updated_at_utc(&self) -> DateTime<Utc> {
        self.updated_at_utc
    }

    pub fn source_device_id(&self) -> Uuid {
        self.source_device_id
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn deleted_at_utc(&self) -> Option<DateTime<Utc>> {
        self.deleted_at_utc
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DailyPlanItemStatus {
    Planned,
    InProgress,
    Completed,
    Incomplete,
    Deferred,
}

impl DailyPlanItemStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Incomplete => "incomplete",
            Self::Deferred => "deferred",
        }
    }

    pub fn from_persisted(value: &str) -> AppResult<Self> {
        match value {
            "planned" => Ok(Self::Planned),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "incomplete" => Ok(Self::Incomplete),
            "deferred" => Ok(Self::Deferred),
            _ => Err(AppError::InvalidDailyPlan("item_status")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyPlanItem {
    id: Uuid,
    daily_plan_id: Uuid,
    subject_id: Uuid,
    task_id: Option<Uuid>,
    title: String,
    status: DailyPlanItemStatus,
    sort_order: i64,
    created_at_utc: DateTime<Utc>,
    updated_at_utc: DateTime<Utc>,
    source_device_id: Uuid,
    revision: u64,
    deleted_at_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyPlanItemSnapshot {
    pub id: Uuid,
    pub daily_plan_id: Uuid,
    pub subject_id: Uuid,
    pub task_id: Option<Uuid>,
    pub title: String,
    pub status: DailyPlanItemStatus,
    pub sort_order: i64,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    pub source_device_id: Uuid,
    pub revision: u64,
    pub deleted_at_utc: Option<DateTime<Utc>>,
}

impl DailyPlanItem {
    pub fn create(
        ids: DailyPlanItemIds,
        title: impl Into<String>,
        sort_order: i64,
        now: DateTime<Utc>,
        source_device_id: Uuid,
    ) -> AppResult<Self> {
        Ok(Self {
            id: ids.id,
            daily_plan_id: ids.daily_plan_id,
            subject_id: ids.subject_id,
            task_id: ids.task_id,
            title: normalized_title(title)?,
            status: DailyPlanItemStatus::Planned,
            sort_order,
            created_at_utc: now,
            updated_at_utc: now,
            source_device_id,
            revision: 1,
            deleted_at_utc: None,
        })
    }

    pub fn restore(snapshot: DailyPlanItemSnapshot) -> AppResult<Self> {
        if snapshot.revision == 0
            || snapshot.updated_at_utc < snapshot.created_at_utc
            || snapshot
                .deleted_at_utc
                .is_some_and(|deleted_at| deleted_at < snapshot.created_at_utc)
        {
            return Err(AppError::InvalidDailyPlan("item_metadata"));
        }
        Ok(Self {
            id: snapshot.id,
            daily_plan_id: snapshot.daily_plan_id,
            subject_id: snapshot.subject_id,
            task_id: snapshot.task_id,
            title: normalized_title(snapshot.title)?,
            status: snapshot.status,
            sort_order: snapshot.sort_order,
            created_at_utc: snapshot.created_at_utc,
            updated_at_utc: snapshot.updated_at_utc,
            source_device_id: snapshot.source_device_id,
            revision: snapshot.revision,
            deleted_at_utc: snapshot.deleted_at_utc,
        })
    }

    pub fn rename(&mut self, title: impl Into<String>, now: DateTime<Utc>) -> AppResult<bool> {
        let title = normalized_title(title)?;
        if self.title == title {
            return Ok(false);
        }
        self.mark_changed(now)?;
        self.title = title;
        Ok(true)
    }

    pub fn reorder(&mut self, sort_order: i64, now: DateTime<Utc>) -> AppResult<bool> {
        if self.sort_order == sort_order {
            return Ok(false);
        }
        self.mark_changed(now)?;
        self.sort_order = sort_order;
        Ok(true)
    }

    pub fn set_status(
        &mut self,
        status: DailyPlanItemStatus,
        now: DateTime<Utc>,
    ) -> AppResult<bool> {
        if self.status == status {
            return Ok(false);
        }
        self.mark_changed(now)?;
        self.status = status;
        Ok(true)
    }

    pub fn ensure_revision(&self, expected_revision: u64) -> AppResult<()> {
        if self.revision != expected_revision {
            return Err(AppError::RevisionConflict);
        }
        Ok(())
    }

    fn mark_changed(&mut self, now: DateTime<Utc>) -> AppResult<()> {
        if now < self.created_at_utc || now < self.updated_at_utc {
            return Err(AppError::InvalidDailyPlan("item_updated_at_utc"));
        }
        let next_revision = self
            .revision
            .checked_add(1)
            .ok_or(AppError::InvalidDailyPlan("item_revision"))?;
        self.updated_at_utc = now;
        self.revision = next_revision;
        Ok(())
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn daily_plan_id(&self) -> Uuid {
        self.daily_plan_id
    }

    pub fn subject_id(&self) -> Uuid {
        self.subject_id
    }

    pub fn task_id(&self) -> Option<Uuid> {
        self.task_id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn status(&self) -> DailyPlanItemStatus {
        self.status
    }

    pub fn sort_order(&self) -> i64 {
        self.sort_order
    }

    pub fn created_at_utc(&self) -> DateTime<Utc> {
        self.created_at_utc
    }

    pub fn updated_at_utc(&self) -> DateTime<Utc> {
        self.updated_at_utc
    }

    pub fn source_device_id(&self) -> Uuid {
        self.source_device_id
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn deleted_at_utc(&self) -> Option<DateTime<Utc>> {
        self.deleted_at_utc
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DailyPlanItemIds {
    pub id: Uuid,
    pub daily_plan_id: Uuid,
    pub subject_id: Uuid,
    pub task_id: Option<Uuid>,
}

fn normalized_title(title: impl Into<String>) -> AppResult<String> {
    let title = title.into().trim().to_owned();
    if title.is_empty() {
        return Err(AppError::InvalidDailyPlan("item_title"));
    }
    Ok(title)
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, NaiveDate, TimeZone, Utc};
    use chrono_tz::Asia::Taipei;
    use uuid::Uuid;

    use super::{DailyPlan, DailyPlanItem, DailyPlanItemIds, DailyPlanItemStatus};

    #[test]
    fn 計畫保留原日期與時區() {
        let date = NaiveDate::from_ymd_opt(2026, 7, 23).unwrap();
        let plan = DailyPlan::create(
            Uuid::now_v7(),
            date,
            Taipei,
            Utc.with_ymd_and_hms(2026, 7, 23, 8, 0, 0).unwrap(),
            Uuid::now_v7(),
        );
        assert_eq!(plan.local_date(), date);
        assert_eq!(plan.time_zone(), Taipei);
        assert_eq!(plan.revision(), 1);
    }

    #[test]
    fn 項目狀態明確變更且延期不改變所屬計畫() {
        let plan_id = Uuid::now_v7();
        let now = Utc.with_ymd_and_hms(2026, 7, 23, 8, 0, 0).unwrap();
        let mut item = DailyPlanItem::create(
            DailyPlanItemIds {
                id: Uuid::now_v7(),
                daily_plan_id: plan_id,
                subject_id: Uuid::now_v7(),
                task_id: None,
            },
            "複習數學",
            0,
            now,
            Uuid::now_v7(),
        )
        .unwrap();
        assert!(
            item.set_status(DailyPlanItemStatus::InProgress, now + Duration::minutes(1))
                .unwrap()
        );
        assert!(
            item.set_status(DailyPlanItemStatus::Deferred, now + Duration::minutes(2))
                .unwrap()
        );
        assert_eq!(item.daily_plan_id(), plan_id);
        assert_eq!(item.status(), DailyPlanItemStatus::Deferred);
    }
}
