use std::sync::Arc;

use uuid::Uuid;

use crate::{
    domain::{id::IdGenerator, subject::Subject, time::Clock},
    error::{AppError, AppResult},
    ports::repository::SubjectRepository,
};

pub struct SubjectService {
    repository: Arc<dyn SubjectRepository>,
    id_generator: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
}

impl SubjectService {
    pub fn new(
        repository: Arc<dyn SubjectRepository>,
        id_generator: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repository,
            id_generator,
            clock,
        }
    }

    pub fn create(
        &self,
        name: impl Into<String>,
        sort_order: i64,
        source_device_id: Uuid,
    ) -> AppResult<Subject> {
        let subject = Subject::create(
            self.id_generator.next_id(),
            name,
            sort_order,
            self.clock.now_utc(),
            source_device_id,
        )?;
        self.repository.insert(&subject)?;
        Ok(subject)
    }

    pub fn rename(
        &self,
        id: Uuid,
        expected_revision: u64,
        name: impl Into<String>,
    ) -> AppResult<Subject> {
        let mut subject = self.load_at_revision(id, expected_revision)?;
        if subject.rename(name, self.clock.now_utc())? {
            self.repository.update(&subject, expected_revision)?;
        }
        Ok(subject)
    }

    pub fn reorder(&self, id: Uuid, expected_revision: u64, sort_order: i64) -> AppResult<Subject> {
        let mut subject = self.load_at_revision(id, expected_revision)?;
        if subject.reorder(sort_order, self.clock.now_utc())? {
            self.repository.update(&subject, expected_revision)?;
        }
        Ok(subject)
    }

    pub fn archive(&self, id: Uuid, expected_revision: u64) -> AppResult<Subject> {
        let mut subject = self.load_at_revision(id, expected_revision)?;
        if subject.archive(self.clock.now_utc())? {
            self.repository.update(&subject, expected_revision)?;
        }
        Ok(subject)
    }

    pub fn list(&self, include_archived: bool) -> AppResult<Vec<Subject>> {
        self.repository.list(include_archived)
    }

    pub fn ensure_can_create_task(&self, id: Uuid) -> AppResult<()> {
        self.load(id)?.ensure_accepts_new_work()
    }

    pub fn ensure_can_start_session(&self, id: Uuid) -> AppResult<()> {
        self.load(id)?.ensure_accepts_new_work()
    }

    fn load(&self, id: Uuid) -> AppResult<Subject> {
        self.repository.get(id)?.ok_or(AppError::SubjectNotFound)
    }

    fn load_at_revision(&self, id: Uuid, expected_revision: u64) -> AppResult<Subject> {
        let subject = self.load(id)?;
        subject.ensure_revision(expected_revision)?;
        Ok(subject)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Instant,
    };

    use chrono::{DateTime, TimeZone, Utc};
    use uuid::Uuid;

    use super::SubjectService;
    use crate::{
        domain::{id::UuidV7Generator, subject::Subject, time::Clock},
        error::{AppError, AppResult},
        ports::repository::SubjectRepository,
    };

    #[derive(Default)]
    struct MemorySubjectRepository {
        subjects: Mutex<Vec<Subject>>,
    }

    impl SubjectRepository for MemorySubjectRepository {
        fn insert(&self, subject: &Subject) -> AppResult<()> {
            self.subjects.lock().unwrap().push(subject.clone());
            Ok(())
        }

        fn get(&self, id: Uuid) -> AppResult<Option<Subject>> {
            Ok(self
                .subjects
                .lock()
                .unwrap()
                .iter()
                .find(|subject| subject.id() == id)
                .cloned())
        }

        fn list(&self, include_archived: bool) -> AppResult<Vec<Subject>> {
            let mut subjects: Vec<_> = self
                .subjects
                .lock()
                .unwrap()
                .iter()
                .filter(|subject| include_archived || subject.archived_at_utc().is_none())
                .cloned()
                .collect();
            subjects.sort_by_key(|subject| (subject.sort_order(), subject.name().to_owned()));
            Ok(subjects)
        }

        fn update(&self, subject: &Subject, expected_revision: u64) -> AppResult<()> {
            let mut subjects = self.subjects.lock().unwrap();
            let stored = subjects
                .iter_mut()
                .find(|stored| stored.id() == subject.id())
                .ok_or(AppError::SubjectNotFound)?;
            if stored.revision() != expected_revision {
                return Err(AppError::RevisionConflict);
            }
            *stored = subject.clone();
            Ok(())
        }
    }

    struct FakeClock(DateTime<Utc>);

    impl Clock for FakeClock {
        fn now_utc(&self) -> DateTime<Utc> {
            self.0
        }

        fn monotonic_now(&self) -> Instant {
            Instant::now()
        }
    }

    fn service() -> (SubjectService, Arc<MemorySubjectRepository>, Uuid) {
        let repository = Arc::new(MemorySubjectRepository::default());
        let source_device_id = Uuid::now_v7();
        let service = SubjectService::new(
            repository.clone(),
            Arc::new(UuidV7Generator),
            Arc::new(FakeClock(
                Utc.with_ymd_and_hms(2026, 7, 23, 8, 0, 0).unwrap(),
            )),
        );
        (service, repository, source_device_id)
    }

    #[test]
    fn 可建立編輯排序與封存科目() {
        let (service, _, source_device_id) = service();
        let mathematics = service.create("數學", 10, source_device_id).unwrap();
        let english = service.create("英文", 0, source_device_id).unwrap();
        assert_eq!(service.list(false).unwrap()[0].id(), english.id());

        let mathematics = service
            .rename(mathematics.id(), mathematics.revision(), "進階數學")
            .unwrap();
        let mathematics = service
            .reorder(mathematics.id(), mathematics.revision(), -1)
            .unwrap();
        assert_eq!(service.list(false).unwrap()[0].id(), mathematics.id());

        let archived = service
            .archive(mathematics.id(), mathematics.revision())
            .unwrap();
        assert!(
            service
                .list(false)
                .unwrap()
                .iter()
                .all(|item| item.id() != archived.id())
        );
        assert!(
            service
                .list(true)
                .unwrap()
                .iter()
                .any(|item| item.id() == archived.id())
        );
        assert!(matches!(
            service.ensure_can_create_task(archived.id()),
            Err(AppError::SubjectArchived)
        ));
        assert!(matches!(
            service.ensure_can_start_session(archived.id()),
            Err(AppError::SubjectArchived)
        ));
    }

    #[test]
    fn 拒絕使用過期版本覆寫科目() {
        let (service, _, source_device_id) = service();
        let subject = service.create("數學", 0, source_device_id).unwrap();
        service
            .rename(subject.id(), subject.revision(), "進階數學")
            .unwrap();
        assert!(matches!(
            service.reorder(subject.id(), subject.revision(), 1),
            Err(AppError::RevisionConflict)
        ));
    }
}
