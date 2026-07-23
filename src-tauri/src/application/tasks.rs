use std::sync::Arc;

use uuid::Uuid;

use crate::{
    domain::{id::IdGenerator, study_task::StudyTask, subject::Subject, time::Clock},
    error::{AppError, AppResult},
    ports::repository::{SubjectRepository, TaskRepository},
};

pub struct TaskService {
    task_repository: Arc<dyn TaskRepository>,
    subject_repository: Arc<dyn SubjectRepository>,
    id_generator: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
}

impl TaskService {
    pub fn new(
        task_repository: Arc<dyn TaskRepository>,
        subject_repository: Arc<dyn SubjectRepository>,
        id_generator: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            task_repository,
            subject_repository,
            id_generator,
            clock,
        }
    }

    pub fn create(
        &self,
        subject_id: Uuid,
        title: impl Into<String>,
        source_device_id: Uuid,
    ) -> AppResult<StudyTask> {
        self.load_subject(subject_id)?.ensure_accepts_new_work()?;
        let task = StudyTask::create(
            self.id_generator.next_id(),
            subject_id,
            title,
            self.clock.now_utc(),
            source_device_id,
        )?;
        self.task_repository.insert(&task)?;
        Ok(task)
    }

    pub fn rename(
        &self,
        id: Uuid,
        expected_revision: u64,
        title: impl Into<String>,
    ) -> AppResult<StudyTask> {
        let mut task = self.load_at_revision(id, expected_revision)?;
        if task.rename(title, self.clock.now_utc())? {
            self.task_repository.update(&task, expected_revision)?;
        }
        Ok(task)
    }

    pub fn complete(&self, id: Uuid, expected_revision: u64) -> AppResult<StudyTask> {
        let mut task = self.load_at_revision(id, expected_revision)?;
        if task.complete(self.clock.now_utc())? {
            self.task_repository.update(&task, expected_revision)?;
        }
        Ok(task)
    }

    pub fn reopen(&self, id: Uuid, expected_revision: u64) -> AppResult<StudyTask> {
        let mut task = self.load_at_revision(id, expected_revision)?;
        if task.reopen(self.clock.now_utc())? {
            self.task_repository.update(&task, expected_revision)?;
        }
        Ok(task)
    }

    pub fn archive(&self, id: Uuid, expected_revision: u64) -> AppResult<StudyTask> {
        let mut task = self.load_at_revision(id, expected_revision)?;
        if task.archive(self.clock.now_utc())? {
            self.task_repository.update(&task, expected_revision)?;
        }
        Ok(task)
    }

    pub fn list_by_subject(
        &self,
        subject_id: Uuid,
        include_archived: bool,
    ) -> AppResult<Vec<StudyTask>> {
        self.task_repository
            .list_by_subject(subject_id, include_archived)
    }

    pub fn ensure_can_start_session(&self, id: Uuid) -> AppResult<()> {
        let task = self.load(id)?;
        task.ensure_accepts_new_session()?;
        self.load_subject(task.subject_id())?
            .ensure_accepts_new_work()
    }

    fn load(&self, id: Uuid) -> AppResult<StudyTask> {
        self.task_repository.get(id)?.ok_or(AppError::TaskNotFound)
    }

    fn load_at_revision(&self, id: Uuid, expected_revision: u64) -> AppResult<StudyTask> {
        let task = self.load(id)?;
        task.ensure_revision(expected_revision)?;
        Ok(task)
    }

    fn load_subject(&self, id: Uuid) -> AppResult<Subject> {
        self.subject_repository
            .get(id)?
            .ok_or(AppError::SubjectNotFound)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Instant,
    };

    use chrono::{DateTime, Duration, TimeZone, Utc};
    use uuid::Uuid;

    use super::TaskService;
    use crate::{
        domain::{
            id::UuidV7Generator,
            study_task::{StudyTask, TaskStatus},
            subject::Subject,
            time::Clock,
        },
        error::{AppError, AppResult},
        ports::repository::{SubjectRepository, TaskRepository},
    };

    #[derive(Default)]
    struct MemoryRepositories {
        subjects: Mutex<Vec<Subject>>,
        tasks: Mutex<Vec<StudyTask>>,
    }

    impl SubjectRepository for MemoryRepositories {
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
            Ok(self
                .subjects
                .lock()
                .unwrap()
                .iter()
                .filter(|subject| include_archived || subject.archived_at_utc().is_none())
                .cloned()
                .collect())
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

    impl TaskRepository for MemoryRepositories {
        fn insert(&self, task: &StudyTask) -> AppResult<()> {
            self.tasks.lock().unwrap().push(task.clone());
            Ok(())
        }

        fn get(&self, id: Uuid) -> AppResult<Option<StudyTask>> {
            Ok(self
                .tasks
                .lock()
                .unwrap()
                .iter()
                .find(|task| task.id() == id)
                .cloned())
        }

        fn list_by_subject(
            &self,
            subject_id: Uuid,
            include_archived: bool,
        ) -> AppResult<Vec<StudyTask>> {
            Ok(self
                .tasks
                .lock()
                .unwrap()
                .iter()
                .filter(|task| task.subject_id() == subject_id)
                .filter(|task| include_archived || task.status() != TaskStatus::Archived)
                .cloned()
                .collect())
        }

        fn update(&self, task: &StudyTask, expected_revision: u64) -> AppResult<()> {
            let mut tasks = self.tasks.lock().unwrap();
            let stored = tasks
                .iter_mut()
                .find(|stored| stored.id() == task.id())
                .ok_or(AppError::TaskNotFound)?;
            if stored.revision() != expected_revision {
                return Err(AppError::RevisionConflict);
            }
            *stored = task.clone();
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

    struct Fixture {
        service: TaskService,
        repositories: Arc<MemoryRepositories>,
        subject: Subject,
        source_device_id: Uuid,
        now: DateTime<Utc>,
    }

    fn fixture() -> Fixture {
        let repositories = Arc::new(MemoryRepositories::default());
        let source_device_id = Uuid::now_v7();
        let now = Utc.with_ymd_and_hms(2026, 7, 23, 8, 0, 0).unwrap();
        let subject = Subject::create(Uuid::now_v7(), "數學", 0, now, source_device_id).unwrap();
        SubjectRepository::insert(repositories.as_ref(), &subject).unwrap();
        let service = TaskService::new(
            repositories.clone(),
            repositories.clone(),
            Arc::new(UuidV7Generator),
            Arc::new(FakeClock(now)),
        );
        Fixture {
            service,
            repositories,
            subject,
            source_device_id,
            now,
        }
    }

    #[test]
    fn 可建立編輯完成重新開啟與封存任務() {
        let fixture = fixture();
        let task = fixture
            .service
            .create(fixture.subject.id(), "練習第一章", fixture.source_device_id)
            .unwrap();
        let task = fixture
            .service
            .rename(task.id(), task.revision(), "練習第二章")
            .unwrap();
        let task = fixture
            .service
            .complete(task.id(), task.revision())
            .unwrap();
        assert_eq!(task.status(), TaskStatus::Completed);
        let task = fixture.service.reopen(task.id(), task.revision()).unwrap();
        assert_eq!(task.status(), TaskStatus::Open);
        let task = fixture.service.archive(task.id(), task.revision()).unwrap();
        assert!(
            fixture
                .service
                .list_by_subject(fixture.subject.id(), false)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            fixture
                .service
                .list_by_subject(fixture.subject.id(), true)
                .unwrap()[0]
                .id(),
            task.id()
        );
    }

    #[test]
    fn 封存科目拒絕新任務及其任務的新工作階段() {
        let fixture = fixture();
        let task = fixture
            .service
            .create(fixture.subject.id(), "練習第一章", fixture.source_device_id)
            .unwrap();
        let mut archived_subject = fixture.subject.clone();
        archived_subject
            .archive(fixture.now + Duration::minutes(1))
            .unwrap();
        SubjectRepository::update(
            fixture.repositories.as_ref(),
            &archived_subject,
            fixture.subject.revision(),
        )
        .unwrap();

        assert!(matches!(
            fixture.service.create(
                archived_subject.id(),
                "練習第二章",
                fixture.source_device_id
            ),
            Err(AppError::SubjectArchived)
        ));
        assert!(matches!(
            fixture.service.ensure_can_start_session(task.id()),
            Err(AppError::SubjectArchived)
        ));
    }

    #[test]
    fn 拒絕不存在科目與過期任務版本() {
        let fixture = fixture();
        assert!(matches!(
            fixture
                .service
                .create(Uuid::now_v7(), "無科目任務", fixture.source_device_id),
            Err(AppError::SubjectNotFound)
        ));
        let task = fixture
            .service
            .create(fixture.subject.id(), "練習第一章", fixture.source_device_id)
            .unwrap();
        fixture
            .service
            .complete(task.id(), task.revision())
            .unwrap();
        assert!(matches!(
            fixture
                .service
                .rename(task.id(), task.revision(), "過期修改"),
            Err(AppError::RevisionConflict)
        ));
    }
}
