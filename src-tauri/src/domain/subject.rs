use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    id: Uuid,
    name: String,
    sort_order: i64,
    archived_at_utc: Option<DateTime<Utc>>,
    created_at_utc: DateTime<Utc>,
    updated_at_utc: DateTime<Utc>,
    source_device_id: Uuid,
    revision: u64,
    deleted_at_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectSnapshot {
    pub id: Uuid,
    pub name: String,
    pub sort_order: i64,
    pub archived_at_utc: Option<DateTime<Utc>>,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    pub source_device_id: Uuid,
    pub revision: u64,
    pub deleted_at_utc: Option<DateTime<Utc>>,
}

impl Subject {
    pub fn create(
        id: Uuid,
        name: impl Into<String>,
        sort_order: i64,
        now: DateTime<Utc>,
        source_device_id: Uuid,
    ) -> AppResult<Self> {
        let name = normalized_name(name)?;
        Ok(Self {
            id,
            name,
            sort_order,
            archived_at_utc: None,
            created_at_utc: now,
            updated_at_utc: now,
            source_device_id,
            revision: 1,
            deleted_at_utc: None,
        })
    }

    pub fn restore(snapshot: SubjectSnapshot) -> AppResult<Self> {
        if snapshot.revision == 0 {
            return Err(AppError::InvalidSubject("revision"));
        }
        if snapshot.updated_at_utc < snapshot.created_at_utc
            || snapshot
                .archived_at_utc
                .is_some_and(|archived_at| archived_at < snapshot.created_at_utc)
            || snapshot
                .deleted_at_utc
                .is_some_and(|deleted_at| deleted_at < snapshot.created_at_utc)
        {
            return Err(AppError::InvalidSubject("timestamp"));
        }
        Ok(Self {
            name: normalized_name(snapshot.name)?,
            id: snapshot.id,
            sort_order: snapshot.sort_order,
            archived_at_utc: snapshot.archived_at_utc,
            created_at_utc: snapshot.created_at_utc,
            updated_at_utc: snapshot.updated_at_utc,
            source_device_id: snapshot.source_device_id,
            revision: snapshot.revision,
            deleted_at_utc: snapshot.deleted_at_utc,
        })
    }

    pub fn rename(&mut self, name: impl Into<String>, now: DateTime<Utc>) -> AppResult<bool> {
        let name = normalized_name(name)?;
        if self.name == name {
            return Ok(false);
        }
        self.mark_changed(now)?;
        self.name = name;
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

    pub fn archive(&mut self, now: DateTime<Utc>) -> AppResult<bool> {
        if self.archived_at_utc.is_some() {
            return Ok(false);
        }
        self.mark_changed(now)?;
        self.archived_at_utc = Some(now);
        Ok(true)
    }

    pub fn ensure_accepts_new_work(&self) -> AppResult<()> {
        if self.archived_at_utc.is_some() || self.deleted_at_utc.is_some() {
            return Err(AppError::SubjectArchived);
        }
        Ok(())
    }

    pub fn ensure_revision(&self, expected_revision: u64) -> AppResult<()> {
        if self.revision != expected_revision {
            return Err(AppError::RevisionConflict);
        }
        Ok(())
    }

    fn mark_changed(&mut self, now: DateTime<Utc>) -> AppResult<()> {
        self.ensure_timestamp_not_before_creation(now)?;
        if now < self.updated_at_utc {
            return Err(AppError::InvalidSubject("updated_at_utc"));
        }
        let next_revision = self
            .revision
            .checked_add(1)
            .ok_or(AppError::InvalidSubject("revision"))?;
        self.updated_at_utc = now;
        self.revision = next_revision;
        Ok(())
    }

    fn ensure_timestamp_not_before_creation(&self, now: DateTime<Utc>) -> AppResult<()> {
        if now < self.created_at_utc {
            return Err(AppError::InvalidSubject("timestamp"));
        }
        Ok(())
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn sort_order(&self) -> i64 {
        self.sort_order
    }

    pub fn archived_at_utc(&self) -> Option<DateTime<Utc>> {
        self.archived_at_utc
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

fn normalized_name(name: impl Into<String>) -> AppResult<String> {
    let name = name.into().trim().to_owned();
    if name.is_empty() {
        return Err(AppError::InvalidSubject("name"));
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use uuid::Uuid;

    use super::Subject;
    use crate::error::AppError;

    fn subject() -> Subject {
        Subject::create(
            Uuid::now_v7(),
            " 數學 ",
            0,
            Utc.with_ymd_and_hms(2026, 7, 23, 8, 0, 0).unwrap(),
            Uuid::now_v7(),
        )
        .unwrap()
    }

    #[test]
    fn 建立時整理名稱並保留生命週期欄位() {
        let subject = subject();
        assert_eq!(subject.name(), "數學");
        assert_eq!(subject.revision(), 1);
        assert!(subject.archived_at_utc().is_none());
        assert!(subject.ensure_accepts_new_work().is_ok());
    }

    #[test]
    fn 拒絕空白名稱且相同修改不增加版本() {
        let mut subject = subject();
        let now = subject.created_at_utc() + Duration::minutes(1);
        assert!(matches!(
            subject.rename("   ", now),
            Err(AppError::InvalidSubject("name"))
        ));
        assert!(!subject.rename("數學", now).unwrap());
        assert!(!subject.reorder(0, now).unwrap());
        assert_eq!(subject.revision(), 1);
    }

    #[test]
    fn 封存後保留資料並拒絕新工作() {
        let mut subject = subject();
        let now = subject.created_at_utc() + Duration::minutes(1);
        assert!(subject.archive(now).unwrap());
        assert_eq!(subject.archived_at_utc(), Some(now));
        assert_eq!(subject.revision(), 2);
        assert!(matches!(
            subject.ensure_accepts_new_work(),
            Err(AppError::SubjectArchived)
        ));
        assert!(!subject.archive(now).unwrap());
        assert_eq!(subject.revision(), 2);
    }
}
