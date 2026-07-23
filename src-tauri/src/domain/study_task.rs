use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Open,
    Completed,
    Archived,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Completed => "completed",
            Self::Archived => "archived",
        }
    }

    pub fn from_persisted(value: &str) -> AppResult<Self> {
        match value {
            "open" => Ok(Self::Open),
            "completed" => Ok(Self::Completed),
            "archived" => Ok(Self::Archived),
            _ => Err(AppError::InvalidTask("status")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudyTask {
    id: Uuid,
    subject_id: Uuid,
    title: String,
    status: TaskStatus,
    created_at_utc: DateTime<Utc>,
    updated_at_utc: DateTime<Utc>,
    source_device_id: Uuid,
    revision: u64,
    deleted_at_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudyTaskSnapshot {
    pub id: Uuid,
    pub subject_id: Uuid,
    pub title: String,
    pub status: TaskStatus,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    pub source_device_id: Uuid,
    pub revision: u64,
    pub deleted_at_utc: Option<DateTime<Utc>>,
}

impl StudyTask {
    pub fn create(
        id: Uuid,
        subject_id: Uuid,
        title: impl Into<String>,
        now: DateTime<Utc>,
        source_device_id: Uuid,
    ) -> AppResult<Self> {
        Ok(Self {
            id,
            subject_id,
            title: normalized_title(title)?,
            status: TaskStatus::Open,
            created_at_utc: now,
            updated_at_utc: now,
            source_device_id,
            revision: 1,
            deleted_at_utc: None,
        })
    }

    pub fn restore(snapshot: StudyTaskSnapshot) -> AppResult<Self> {
        if snapshot.revision == 0 {
            return Err(AppError::InvalidTask("revision"));
        }
        if snapshot.updated_at_utc < snapshot.created_at_utc
            || snapshot
                .deleted_at_utc
                .is_some_and(|deleted_at| deleted_at < snapshot.created_at_utc)
        {
            return Err(AppError::InvalidTask("timestamp"));
        }
        Ok(Self {
            id: snapshot.id,
            subject_id: snapshot.subject_id,
            title: normalized_title(snapshot.title)?,
            status: snapshot.status,
            created_at_utc: snapshot.created_at_utc,
            updated_at_utc: snapshot.updated_at_utc,
            source_device_id: snapshot.source_device_id,
            revision: snapshot.revision,
            deleted_at_utc: snapshot.deleted_at_utc,
        })
    }

    pub fn rename(&mut self, title: impl Into<String>, now: DateTime<Utc>) -> AppResult<bool> {
        self.ensure_not_archived()?;
        let title = normalized_title(title)?;
        if self.title == title {
            return Ok(false);
        }
        self.mark_changed(now)?;
        self.title = title;
        Ok(true)
    }

    pub fn complete(&mut self, now: DateTime<Utc>) -> AppResult<bool> {
        self.ensure_not_archived()?;
        if self.status == TaskStatus::Completed {
            return Ok(false);
        }
        self.mark_changed(now)?;
        self.status = TaskStatus::Completed;
        Ok(true)
    }

    pub fn reopen(&mut self, now: DateTime<Utc>) -> AppResult<bool> {
        self.ensure_not_archived()?;
        if self.status == TaskStatus::Open {
            return Ok(false);
        }
        self.mark_changed(now)?;
        self.status = TaskStatus::Open;
        Ok(true)
    }

    pub fn archive(&mut self, now: DateTime<Utc>) -> AppResult<bool> {
        if self.status == TaskStatus::Archived {
            return Ok(false);
        }
        self.mark_changed(now)?;
        self.status = TaskStatus::Archived;
        Ok(true)
    }

    pub fn ensure_accepts_new_session(&self) -> AppResult<()> {
        if self.status == TaskStatus::Archived || self.deleted_at_utc.is_some() {
            return Err(AppError::TaskArchived);
        }
        Ok(())
    }

    pub fn ensure_revision(&self, expected_revision: u64) -> AppResult<()> {
        if self.revision != expected_revision {
            return Err(AppError::RevisionConflict);
        }
        Ok(())
    }

    fn ensure_not_archived(&self) -> AppResult<()> {
        if self.status == TaskStatus::Archived || self.deleted_at_utc.is_some() {
            return Err(AppError::TaskArchived);
        }
        Ok(())
    }

    fn mark_changed(&mut self, now: DateTime<Utc>) -> AppResult<()> {
        if now < self.created_at_utc || now < self.updated_at_utc {
            return Err(AppError::InvalidTask("updated_at_utc"));
        }
        let next_revision = self
            .revision
            .checked_add(1)
            .ok_or(AppError::InvalidTask("revision"))?;
        self.updated_at_utc = now;
        self.revision = next_revision;
        Ok(())
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn subject_id(&self) -> Uuid {
        self.subject_id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn status(&self) -> TaskStatus {
        self.status
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

fn normalized_title(title: impl Into<String>) -> AppResult<String> {
    let title = title.into().trim().to_owned();
    if title.is_empty() {
        return Err(AppError::InvalidTask("title"));
    }
    Ok(title)
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use uuid::Uuid;

    use super::{StudyTask, TaskStatus};
    use crate::error::AppError;

    fn task() -> StudyTask {
        StudyTask::create(
            Uuid::now_v7(),
            Uuid::now_v7(),
            " 練習第一章 ",
            Utc.with_ymd_and_hms(2026, 7, 23, 8, 0, 0).unwrap(),
            Uuid::now_v7(),
        )
        .unwrap()
    }

    #[test]
    fn 建立時整理標題並固定所屬科目() {
        let task = task();
        assert_eq!(task.title(), "練習第一章");
        assert_eq!(task.status(), TaskStatus::Open);
        assert_eq!(task.revision(), 1);
    }

    #[test]
    fn 可完成與重新開啟且不受日期綁定() {
        let mut task = task();
        let subject_id = task.subject_id();
        let now = task.created_at_utc() + Duration::minutes(1);
        assert!(task.complete(now).unwrap());
        assert_eq!(task.status(), TaskStatus::Completed);
        assert!(task.reopen(now).unwrap());
        assert_eq!(task.status(), TaskStatus::Open);
        assert_eq!(task.subject_id(), subject_id);
        assert_eq!(task.revision(), 3);
    }

    #[test]
    fn 封存後保留資料並拒絕編輯與新工作階段() {
        let mut task = task();
        let now = task.created_at_utc() + Duration::minutes(1);
        assert!(task.archive(now).unwrap());
        assert_eq!(task.status(), TaskStatus::Archived);
        assert!(matches!(
            task.rename("新名稱", now),
            Err(AppError::TaskArchived)
        ));
        assert!(matches!(task.complete(now), Err(AppError::TaskArchived)));
        assert!(matches!(task.reopen(now), Err(AppError::TaskArchived)));
        assert!(matches!(
            task.ensure_accepts_new_session(),
            Err(AppError::TaskArchived)
        ));
    }

    #[test]
    fn 拒絕空白標題且相同修改不增加版本() {
        let mut task = task();
        let now = task.created_at_utc() + Duration::minutes(1);
        assert!(matches!(
            task.rename("   ", now),
            Err(AppError::InvalidTask("title"))
        ));
        assert!(!task.rename("練習第一章", now).unwrap());
        assert_eq!(task.revision(), 1);
    }
}
