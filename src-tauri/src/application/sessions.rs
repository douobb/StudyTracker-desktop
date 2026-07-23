use std::sync::Arc;

use chrono_tz::Tz;
use uuid::Uuid;

use crate::{
    domain::{
        daily_plan::{DailyPlanItem, DailyPlanItemStatus},
        id::IdGenerator,
        session::{Session, SessionIds, TimerMode},
        study_task::StudyTask,
        subject::Subject,
        time::Clock,
    },
    error::{AppError, AppResult},
    ports::repository::{
        DailyPlanRepository, SessionRepository, SubjectRepository, TaskRepository,
    },
};

pub struct SessionService {
    session_repository: Arc<dyn SessionRepository>,
    subject_repository: Arc<dyn SubjectRepository>,
    task_repository: Arc<dyn TaskRepository>,
    daily_plan_repository: Arc<dyn DailyPlanRepository>,
    id_generator: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
}

impl SessionService {
    pub fn new(
        session_repository: Arc<dyn SessionRepository>,
        subject_repository: Arc<dyn SubjectRepository>,
        task_repository: Arc<dyn TaskRepository>,
        daily_plan_repository: Arc<dyn DailyPlanRepository>,
        id_generator: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            session_repository,
            subject_repository,
            task_repository,
            daily_plan_repository,
            id_generator,
            clock,
        }
    }

    pub fn start(&self, input: SessionStartInput) -> AppResult<Session> {
        if self.session_repository.get_active()?.is_some() {
            return Err(AppError::ActiveSessionExists);
        }
        self.load_subject(input.subject_id)?
            .ensure_accepts_new_work()?;
        let now = self.clock.now_utc();
        let mut item_update = self.prepare_daily_plan_item(&input, now)?;
        let resolved_task_id = resolve_task_id(input.task_id, item_update.as_ref())?;
        if let Some(task_id) = resolved_task_id {
            let task = self.load_task(task_id)?;
            task.ensure_accepts_new_session()?;
            if task.subject_id() != input.subject_id {
                return Err(AppError::SessionReferenceMismatch);
            }
        }
        let time_zone = input
            .time_zone_id
            .parse::<Tz>()
            .map_err(|_| AppError::InvalidSession("time_zone_id"))?;
        let session = Session::start(
            SessionIds {
                id: self.id_generator.next_id(),
                first_segment_id: self.id_generator.next_id(),
                subject_id: input.subject_id,
                task_id: resolved_task_id,
                daily_plan_item_id: input.daily_plan_item_id,
            },
            input.timer_mode,
            time_zone,
            input.timer_config_json,
            now,
            input.source_device_id,
        )?;
        let started_item = item_update
            .as_mut()
            .and_then(|(item, expected_revision, changed)| {
                changed.then_some((&*item, *expected_revision))
            });
        self.session_repository.create(&session, started_item)?;
        Ok(session)
    }

    pub fn pause(&self, id: Uuid, expected_revision: u64) -> AppResult<Session> {
        self.transition(id, expected_revision, |session, segment_id, now| {
            session.pause(segment_id, now)
        })
    }

    pub fn resume(&self, id: Uuid, expected_revision: u64) -> AppResult<Session> {
        self.transition(id, expected_revision, |session, segment_id, now| {
            session.resume(segment_id, now)
        })
    }

    pub fn start_break(&self, id: Uuid, expected_revision: u64) -> AppResult<Session> {
        self.transition(id, expected_revision, |session, segment_id, now| {
            session.start_break(segment_id, now)
        })
    }

    pub fn end_break(&self, id: Uuid, expected_revision: u64) -> AppResult<Session> {
        self.transition(id, expected_revision, |session, segment_id, now| {
            session.end_break(segment_id, now)
        })
    }

    pub fn end(&self, id: Uuid, expected_revision: u64) -> AppResult<Session> {
        let mut session = self.load_at_revision(id, expected_revision)?;
        session.end(self.clock.now_utc())?;
        self.session_repository.save(&session, expected_revision)?;
        Ok(session)
    }

    pub fn active(&self) -> AppResult<Option<Session>> {
        self.session_repository.get_active()
    }

    fn transition(
        &self,
        id: Uuid,
        expected_revision: u64,
        operation: impl FnOnce(&mut Session, Uuid, chrono::DateTime<chrono::Utc>) -> AppResult<()>,
    ) -> AppResult<Session> {
        let mut session = self.load_at_revision(id, expected_revision)?;
        operation(
            &mut session,
            self.id_generator.next_id(),
            self.clock.now_utc(),
        )?;
        self.session_repository.save(&session, expected_revision)?;
        Ok(session)
    }

    fn prepare_daily_plan_item(
        &self,
        input: &SessionStartInput,
        now: chrono::DateTime<chrono::Utc>,
    ) -> AppResult<Option<(DailyPlanItem, u64, bool)>> {
        let Some(item_id) = input.daily_plan_item_id else {
            return Ok(None);
        };
        let expected_revision = input
            .daily_plan_item_revision
            .ok_or(AppError::InvalidSession("daily_plan_item_revision"))?;
        let mut item = self
            .daily_plan_repository
            .get_item(item_id)?
            .ok_or(AppError::DailyPlanItemNotFound)?;
        item.ensure_revision(expected_revision)?;
        if item.subject_id() != input.subject_id {
            return Err(AppError::SessionReferenceMismatch);
        }
        let changed = item.set_status(DailyPlanItemStatus::InProgress, now)?;
        Ok(Some((item, expected_revision, changed)))
    }

    fn load_subject(&self, id: Uuid) -> AppResult<Subject> {
        self.subject_repository
            .get(id)?
            .ok_or(AppError::SubjectNotFound)
    }

    fn load_task(&self, id: Uuid) -> AppResult<StudyTask> {
        self.task_repository.get(id)?.ok_or(AppError::TaskNotFound)
    }

    fn load_at_revision(&self, id: Uuid, expected_revision: u64) -> AppResult<Session> {
        let session = self
            .session_repository
            .get(id)?
            .ok_or(AppError::SessionNotFound)?;
        session.ensure_revision(expected_revision)?;
        Ok(session)
    }
}

pub struct SessionStartInput {
    pub subject_id: Uuid,
    pub task_id: Option<Uuid>,
    pub daily_plan_item_id: Option<Uuid>,
    pub daily_plan_item_revision: Option<u64>,
    pub timer_mode: TimerMode,
    pub timer_config_json: String,
    pub time_zone_id: String,
    pub source_device_id: Uuid,
}

fn resolve_task_id(
    requested_task_id: Option<Uuid>,
    item_update: Option<&(DailyPlanItem, u64, bool)>,
) -> AppResult<Option<Uuid>> {
    let item_task_id = item_update.and_then(|(item, _, _)| item.task_id());
    match (requested_task_id, item_task_id) {
        (Some(requested), Some(from_item)) if requested != from_item => {
            Err(AppError::SessionReferenceMismatch)
        }
        (Some(requested), _) => Ok(Some(requested)),
        (None, from_item) => Ok(from_item),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Instant,
    };

    use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
    use chrono_tz::Asia::Taipei;
    use uuid::Uuid;

    use super::{SessionService, SessionStartInput};
    use crate::{
        domain::{
            daily_plan::{DailyPlan, DailyPlanItem, DailyPlanItemIds, DailyPlanItemStatus},
            id::UuidV7Generator,
            session::{Session, SessionState, TimerMode},
            study_task::{StudyTask, TaskStatus},
            subject::Subject,
            time::Clock,
        },
        error::{AppError, AppResult},
        ports::repository::{
            DailyPlanRepository, SessionRepository, SubjectRepository, TaskRepository,
        },
    };

    #[derive(Default)]
    struct MemoryRepositories {
        subjects: Mutex<Vec<Subject>>,
        tasks: Mutex<Vec<StudyTask>>,
        plans: Mutex<Vec<DailyPlan>>,
        items: Mutex<Vec<DailyPlanItem>>,
        sessions: Mutex<Vec<Session>>,
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

    impl DailyPlanRepository for MemoryRepositories {
        fn get_or_create(&self, candidate: &DailyPlan) -> AppResult<DailyPlan> {
            self.plans.lock().unwrap().push(candidate.clone());
            Ok(candidate.clone())
        }

        fn get_by_date(
            &self,
            source_device_id: Uuid,
            local_date: NaiveDate,
        ) -> AppResult<Option<DailyPlan>> {
            Ok(self
                .plans
                .lock()
                .unwrap()
                .iter()
                .find(|plan| {
                    plan.source_device_id() == source_device_id && plan.local_date() == local_date
                })
                .cloned())
        }

        fn insert_item(&self, item: &DailyPlanItem) -> AppResult<()> {
            self.items.lock().unwrap().push(item.clone());
            Ok(())
        }

        fn get_item(&self, id: Uuid) -> AppResult<Option<DailyPlanItem>> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|item| item.id() == id)
                .cloned())
        }

        fn list_items(&self, daily_plan_id: Uuid) -> AppResult<Vec<DailyPlanItem>> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.daily_plan_id() == daily_plan_id)
                .cloned()
                .collect())
        }

        fn update_item(&self, item: &DailyPlanItem, expected_revision: u64) -> AppResult<()> {
            let mut items = self.items.lock().unwrap();
            let stored = items
                .iter_mut()
                .find(|stored| stored.id() == item.id())
                .ok_or(AppError::DailyPlanItemNotFound)?;
            if stored.revision() != expected_revision {
                return Err(AppError::RevisionConflict);
            }
            *stored = item.clone();
            Ok(())
        }
    }

    impl SessionRepository for MemoryRepositories {
        fn create(
            &self,
            session: &Session,
            started_item: Option<(&DailyPlanItem, u64)>,
        ) -> AppResult<()> {
            let mut sessions = self.sessions.lock().unwrap();
            if sessions
                .iter()
                .any(|session| session.ended_at_utc().is_none())
            {
                return Err(AppError::ActiveSessionExists);
            }
            if let Some((item, expected_revision)) = started_item {
                let mut items = self.items.lock().unwrap();
                let stored = items
                    .iter_mut()
                    .find(|stored| stored.id() == item.id())
                    .ok_or(AppError::DailyPlanItemNotFound)?;
                if stored.revision() != expected_revision {
                    return Err(AppError::RevisionConflict);
                }
                *stored = item.clone();
            }
            sessions.push(session.clone());
            Ok(())
        }

        fn get(&self, id: Uuid) -> AppResult<Option<Session>> {
            Ok(self
                .sessions
                .lock()
                .unwrap()
                .iter()
                .find(|session| session.id() == id)
                .cloned())
        }

        fn get_active(&self) -> AppResult<Option<Session>> {
            Ok(self
                .sessions
                .lock()
                .unwrap()
                .iter()
                .find(|session| session.ended_at_utc().is_none())
                .cloned())
        }

        fn save(&self, session: &Session, expected_revision: u64) -> AppResult<()> {
            let mut sessions = self.sessions.lock().unwrap();
            let stored = sessions
                .iter_mut()
                .find(|stored| stored.id() == session.id())
                .ok_or(AppError::SessionNotFound)?;
            if stored.revision() != expected_revision {
                return Err(AppError::RevisionConflict);
            }
            *stored = session.clone();
            Ok(())
        }
    }

    struct FakeClock(Mutex<DateTime<Utc>>);

    impl FakeClock {
        fn set(&self, now: DateTime<Utc>) {
            *self.0.lock().unwrap() = now;
        }
    }

    impl Clock for FakeClock {
        fn now_utc(&self) -> DateTime<Utc> {
            *self.0.lock().unwrap()
        }

        fn monotonic_now(&self) -> Instant {
            Instant::now()
        }
    }

    struct Fixture {
        service: SessionService,
        repositories: Arc<MemoryRepositories>,
        clock: Arc<FakeClock>,
        subject: Subject,
        task: StudyTask,
        item: DailyPlanItem,
        device_id: Uuid,
        now: DateTime<Utc>,
    }

    fn fixture() -> Fixture {
        let repositories = Arc::new(MemoryRepositories::default());
        let device_id = Uuid::now_v7();
        let now = Utc.with_ymd_and_hms(2026, 7, 23, 8, 0, 0).unwrap();
        let clock = Arc::new(FakeClock(Mutex::new(now)));
        let subject = Subject::create(Uuid::now_v7(), "數學", 0, now, device_id).unwrap();
        let task =
            StudyTask::create(Uuid::now_v7(), subject.id(), "練習第一章", now, device_id).unwrap();
        let plan = DailyPlan::create(
            Uuid::now_v7(),
            NaiveDate::from_ymd_opt(2026, 7, 23).unwrap(),
            Taipei,
            now,
            device_id,
        );
        let item = DailyPlanItem::create(
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
        .unwrap();
        SubjectRepository::insert(repositories.as_ref(), &subject).unwrap();
        TaskRepository::insert(repositories.as_ref(), &task).unwrap();
        DailyPlanRepository::get_or_create(repositories.as_ref(), &plan).unwrap();
        DailyPlanRepository::insert_item(repositories.as_ref(), &item).unwrap();
        let service = SessionService::new(
            repositories.clone(),
            repositories.clone(),
            repositories.clone(),
            repositories.clone(),
            Arc::new(UuidV7Generator),
            clock.clone(),
        );
        Fixture {
            service,
            repositories,
            clock,
            subject,
            task,
            item,
            device_id,
            now,
        }
    }

    fn start_input(fixture: &Fixture, with_item: bool) -> SessionStartInput {
        SessionStartInput {
            subject_id: fixture.subject.id(),
            task_id: (!with_item).then_some(fixture.task.id()),
            daily_plan_item_id: with_item.then_some(fixture.item.id()),
            daily_plan_item_revision: with_item.then_some(fixture.item.revision()),
            timer_mode: TimerMode::Stopwatch,
            timer_config_json: "{}".to_owned(),
            time_zone_id: "Asia/Taipei".to_owned(),
            source_device_id: fixture.device_id,
        }
    }

    #[test]
    fn 完整轉移只維持一個活動工作階段() {
        let fixture = fixture();
        let mut session = fixture.service.start(start_input(&fixture, false)).unwrap();
        assert!(matches!(
            fixture.service.start(start_input(&fixture, false)),
            Err(AppError::ActiveSessionExists)
        ));
        fixture.clock.set(fixture.now + Duration::minutes(10));
        session = fixture
            .service
            .pause(session.id(), session.revision())
            .unwrap();
        fixture.clock.set(fixture.now + Duration::minutes(11));
        session = fixture
            .service
            .resume(session.id(), session.revision())
            .unwrap();
        fixture.clock.set(fixture.now + Duration::minutes(20));
        session = fixture
            .service
            .start_break(session.id(), session.revision())
            .unwrap();
        fixture.clock.set(fixture.now + Duration::minutes(25));
        session = fixture
            .service
            .end_break(session.id(), session.revision())
            .unwrap();
        fixture.clock.set(fixture.now + Duration::minutes(30));
        session = fixture
            .service
            .end(session.id(), session.revision())
            .unwrap();
        assert_eq!(session.state(), SessionState::Ended);
        assert!(fixture.service.active().unwrap().is_none());
    }

    #[test]
    fn 從計畫項目開始會帶入任務且結束不自動完成() {
        let fixture = fixture();
        let session = fixture.service.start(start_input(&fixture, true)).unwrap();
        assert_eq!(session.task_id(), Some(fixture.task.id()));
        assert_eq!(
            DailyPlanRepository::get_item(fixture.repositories.as_ref(), fixture.item.id())
                .unwrap()
                .unwrap()
                .status(),
            DailyPlanItemStatus::InProgress
        );
        fixture.clock.set(fixture.now + Duration::minutes(30));
        fixture
            .service
            .end(session.id(), session.revision())
            .unwrap();
        assert_eq!(
            DailyPlanRepository::get_item(fixture.repositories.as_ref(), fixture.item.id())
                .unwrap()
                .unwrap()
                .status(),
            DailyPlanItemStatus::InProgress
        );
        assert_eq!(
            TaskRepository::get(fixture.repositories.as_ref(), fixture.task.id())
                .unwrap()
                .unwrap()
                .status(),
            TaskStatus::Open
        );
    }

    #[test]
    fn 封存科目與錯誤關聯無法開始工作階段() {
        let fixture = fixture();
        let mut archived = fixture.subject.clone();
        archived
            .archive(fixture.now + Duration::minutes(1))
            .unwrap();
        SubjectRepository::update(
            fixture.repositories.as_ref(),
            &archived,
            fixture.subject.revision(),
        )
        .unwrap();
        assert!(matches!(
            fixture.service.start(start_input(&fixture, false)),
            Err(AppError::SubjectArchived)
        ));
    }
}
