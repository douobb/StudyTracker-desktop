use std::sync::Arc;

use chrono::NaiveDate;
use chrono_tz::Tz;
use uuid::Uuid;

use crate::{
    domain::{
        daily_plan::{DailyPlan, DailyPlanItem, DailyPlanItemIds, DailyPlanItemStatus},
        id::IdGenerator,
        study_task::StudyTask,
        subject::Subject,
        time::Clock,
    },
    error::{AppError, AppResult},
    ports::repository::{DailyPlanRepository, SubjectRepository, TaskRepository},
};

pub struct DailyPlanService {
    daily_plan_repository: Arc<dyn DailyPlanRepository>,
    subject_repository: Arc<dyn SubjectRepository>,
    task_repository: Arc<dyn TaskRepository>,
    id_generator: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
}

impl DailyPlanService {
    pub fn new(
        daily_plan_repository: Arc<dyn DailyPlanRepository>,
        subject_repository: Arc<dyn SubjectRepository>,
        task_repository: Arc<dyn TaskRepository>,
        id_generator: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            daily_plan_repository,
            subject_repository,
            task_repository,
            id_generator,
            clock,
        }
    }

    pub fn create_item(
        &self,
        plan: DailyPlanInput,
        item: DailyPlanItemInput,
    ) -> AppResult<DailyPlanItem> {
        self.load_subject(item.subject_id)?;
        if let Some(task_id) = item.task_id {
            let task = self.load_task(task_id)?;
            if task.subject_id() != item.subject_id {
                return Err(AppError::DailyPlanReferenceMismatch);
            }
        }
        let daily_plan = self.get_or_create_plan(&plan)?;
        let item = DailyPlanItem::create(
            DailyPlanItemIds {
                id: self.id_generator.next_id(),
                daily_plan_id: daily_plan.id(),
                subject_id: item.subject_id,
                task_id: item.task_id,
            },
            item.title,
            item.sort_order,
            self.clock.now_utc(),
            plan.source_device_id,
        )?;
        self.daily_plan_repository.insert_item(&item)?;
        Ok(item)
    }

    pub fn rename_item(
        &self,
        id: Uuid,
        expected_revision: u64,
        title: impl Into<String>,
    ) -> AppResult<DailyPlanItem> {
        let mut item = self.load_item_at_revision(id, expected_revision)?;
        if item.rename(title, self.clock.now_utc())? {
            self.daily_plan_repository
                .update_item(&item, expected_revision)?;
        }
        Ok(item)
    }

    pub fn reorder_item(
        &self,
        id: Uuid,
        expected_revision: u64,
        sort_order: i64,
    ) -> AppResult<DailyPlanItem> {
        let mut item = self.load_item_at_revision(id, expected_revision)?;
        if item.reorder(sort_order, self.clock.now_utc())? {
            self.daily_plan_repository
                .update_item(&item, expected_revision)?;
        }
        Ok(item)
    }

    pub fn set_item_status(
        &self,
        id: Uuid,
        expected_revision: u64,
        status: DailyPlanItemStatus,
    ) -> AppResult<DailyPlanItem> {
        let mut item = self.load_item_at_revision(id, expected_revision)?;
        if item.set_status(status, self.clock.now_utc())? {
            self.daily_plan_repository
                .update_item(&item, expected_revision)?;
        }
        Ok(item)
    }

    pub fn mark_in_progress_for_session(
        &self,
        id: Uuid,
        expected_revision: u64,
    ) -> AppResult<DailyPlanItem> {
        self.set_item_status(id, expected_revision, DailyPlanItemStatus::InProgress)
    }

    pub fn load_for_date(
        &self,
        source_device_id: Uuid,
        local_date: NaiveDate,
    ) -> AppResult<Option<(DailyPlan, Vec<DailyPlanItem>)>> {
        let Some(plan) = self
            .daily_plan_repository
            .get_by_date(source_device_id, local_date)?
        else {
            return Ok(None);
        };
        let items = self.daily_plan_repository.list_items(plan.id())?;
        Ok(Some((plan, items)))
    }

    fn get_or_create_plan(&self, input: &DailyPlanInput) -> AppResult<DailyPlan> {
        let time_zone = input
            .time_zone_id
            .parse::<Tz>()
            .map_err(|_| AppError::InvalidDailyPlan("time_zone_id"))?;
        let candidate = DailyPlan::create(
            self.id_generator.next_id(),
            input.local_date,
            time_zone,
            self.clock.now_utc(),
            input.source_device_id,
        );
        self.daily_plan_repository.get_or_create(&candidate)
    }

    fn load_subject(&self, id: Uuid) -> AppResult<Subject> {
        self.subject_repository
            .get(id)?
            .ok_or(AppError::SubjectNotFound)
    }

    fn load_task(&self, id: Uuid) -> AppResult<StudyTask> {
        self.task_repository.get(id)?.ok_or(AppError::TaskNotFound)
    }

    fn load_item_at_revision(&self, id: Uuid, expected_revision: u64) -> AppResult<DailyPlanItem> {
        let item = self
            .daily_plan_repository
            .get_item(id)?
            .ok_or(AppError::DailyPlanItemNotFound)?;
        item.ensure_revision(expected_revision)?;
        Ok(item)
    }
}

pub struct DailyPlanInput {
    pub local_date: NaiveDate,
    pub time_zone_id: String,
    pub source_device_id: Uuid,
}

pub struct DailyPlanItemInput {
    pub subject_id: Uuid,
    pub task_id: Option<Uuid>,
    pub title: String,
    pub sort_order: i64,
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Instant,
    };

    use chrono::{DateTime, NaiveDate, TimeZone, Utc};
    use uuid::Uuid;

    use super::{DailyPlanInput, DailyPlanItemInput, DailyPlanService};
    use crate::{
        domain::{
            daily_plan::{DailyPlan, DailyPlanItem, DailyPlanItemStatus},
            id::UuidV7Generator,
            study_task::{StudyTask, TaskStatus},
            subject::Subject,
            time::Clock,
        },
        error::{AppError, AppResult},
        ports::repository::{DailyPlanRepository, SubjectRepository, TaskRepository},
    };

    #[derive(Default)]
    struct MemoryRepositories {
        plans: Mutex<Vec<DailyPlan>>,
        items: Mutex<Vec<DailyPlanItem>>,
        subjects: Mutex<Vec<Subject>>,
        tasks: Mutex<Vec<StudyTask>>,
    }

    impl DailyPlanRepository for MemoryRepositories {
        fn get_or_create(&self, candidate: &DailyPlan) -> AppResult<DailyPlan> {
            let mut plans = self.plans.lock().unwrap();
            if let Some(plan) = plans.iter().find(|plan| {
                plan.source_device_id() == candidate.source_device_id()
                    && plan.local_date() == candidate.local_date()
            }) {
                return Ok(plan.clone());
            }
            plans.push(candidate.clone());
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
            let mut items: Vec<_> = self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.daily_plan_id() == daily_plan_id)
                .cloned()
                .collect();
            items.sort_by_key(|item| (item.sort_order(), item.title().to_owned()));
            Ok(items)
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
        service: DailyPlanService,
        repositories: Arc<MemoryRepositories>,
        subject: Subject,
        task: StudyTask,
        device_id: Uuid,
        date: NaiveDate,
    }

    fn fixture() -> Fixture {
        let repositories = Arc::new(MemoryRepositories::default());
        let device_id = Uuid::now_v7();
        let now = Utc.with_ymd_and_hms(2026, 7, 23, 8, 0, 0).unwrap();
        let subject = Subject::create(Uuid::now_v7(), "數學", 0, now, device_id).unwrap();
        let task =
            StudyTask::create(Uuid::now_v7(), subject.id(), "練習第一章", now, device_id).unwrap();
        SubjectRepository::insert(repositories.as_ref(), &subject).unwrap();
        TaskRepository::insert(repositories.as_ref(), &task).unwrap();
        let service = DailyPlanService::new(
            repositories.clone(),
            repositories.clone(),
            repositories.clone(),
            Arc::new(UuidV7Generator),
            Arc::new(FakeClock(now)),
        );
        Fixture {
            service,
            repositories,
            subject,
            task,
            device_id,
            date: NaiveDate::from_ymd_opt(2026, 7, 23).unwrap(),
        }
    }

    fn plan_input(fixture: &Fixture, date: NaiveDate) -> DailyPlanInput {
        DailyPlanInput {
            local_date: date,
            time_zone_id: "Asia/Taipei".to_owned(),
            source_device_id: fixture.device_id,
        }
    }

    fn item_input(fixture: &Fixture, task_id: Option<Uuid>) -> DailyPlanItemInput {
        DailyPlanItemInput {
            subject_id: fixture.subject.id(),
            task_id,
            title: "今日數學".to_owned(),
            sort_order: 0,
        }
    }

    #[test]
    fn 項目可直接指向科目或引用相符任務() {
        let fixture = fixture();
        fixture
            .service
            .create_item(
                plan_input(&fixture, fixture.date),
                item_input(&fixture, None),
            )
            .unwrap();
        fixture
            .service
            .create_item(
                plan_input(&fixture, fixture.date),
                item_input(&fixture, Some(fixture.task.id())),
            )
            .unwrap();
        let next_date = fixture.date.succ_opt().unwrap();
        fixture
            .service
            .create_item(
                plan_input(&fixture, next_date),
                item_input(&fixture, Some(fixture.task.id())),
            )
            .unwrap();
        let (_, items) = fixture
            .service
            .load_for_date(fixture.device_id, fixture.date)
            .unwrap()
            .unwrap();
        assert_eq!(items.len(), 2);
        let (next_plan, next_items) = fixture
            .service
            .load_for_date(fixture.device_id, next_date)
            .unwrap()
            .unwrap();
        assert_eq!(next_plan.local_date(), next_date);
        assert_eq!(next_items.len(), 1);
    }

    #[test]
    fn 任務與科目不一致時拒絕建立項目() {
        let fixture = fixture();
        let other_subject = Subject::create(
            Uuid::now_v7(),
            "英文",
            1,
            fixture.task.created_at_utc(),
            fixture.device_id,
        )
        .unwrap();
        SubjectRepository::insert(fixture.repositories.as_ref(), &other_subject).unwrap();
        let input = DailyPlanItemInput {
            subject_id: other_subject.id(),
            task_id: Some(fixture.task.id()),
            title: "錯誤關聯".to_owned(),
            sort_order: 0,
        };
        assert!(matches!(
            fixture
                .service
                .create_item(plan_input(&fixture, fixture.date), input),
            Err(AppError::DailyPlanReferenceMismatch)
        ));
    }

    #[test]
    fn 開始工作階段只將項目標為進行中且結束不自動完成() {
        let fixture = fixture();
        let item = fixture
            .service
            .create_item(
                plan_input(&fixture, fixture.date),
                item_input(&fixture, Some(fixture.task.id())),
            )
            .unwrap();
        let item = fixture
            .service
            .mark_in_progress_for_session(item.id(), item.revision())
            .unwrap();
        assert_eq!(item.status(), DailyPlanItemStatus::InProgress);
        assert_eq!(
            TaskRepository::get(fixture.repositories.as_ref(), fixture.task.id())
                .unwrap()
                .unwrap()
                .status(),
            TaskStatus::Open
        );
        assert_eq!(
            fixture
                .service
                .load_for_date(fixture.device_id, fixture.date)
                .unwrap()
                .unwrap()
                .1[0]
                .status(),
            DailyPlanItemStatus::InProgress
        );
    }

    #[test]
    fn 延期保留原日期且不建立新項目或計畫() {
        let fixture = fixture();
        let item = fixture
            .service
            .create_item(
                plan_input(&fixture, fixture.date),
                item_input(&fixture, None),
            )
            .unwrap();
        fixture
            .service
            .set_item_status(item.id(), item.revision(), DailyPlanItemStatus::Deferred)
            .unwrap();
        assert_eq!(fixture.repositories.plans.lock().unwrap().len(), 1);
        assert_eq!(fixture.repositories.items.lock().unwrap().len(), 1);
        assert_eq!(
            fixture.repositories.plans.lock().unwrap()[0].local_date(),
            fixture.date
        );
    }
}
