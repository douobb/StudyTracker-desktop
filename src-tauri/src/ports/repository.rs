use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    domain::{
        daily_plan::{DailyPlan, DailyPlanItem},
        session::Session,
        settings::AppSettings,
        study_task::StudyTask,
        subject::Subject,
    },
    error::AppResult,
};

pub trait SettingsRepository: Send + Sync {
    fn load(&self) -> AppResult<Option<AppSettings>>;
    fn save(&self, settings: &AppSettings) -> AppResult<()>;
}

pub trait SubjectRepository: Send + Sync {
    fn insert(&self, subject: &Subject) -> AppResult<()>;
    fn get(&self, id: Uuid) -> AppResult<Option<Subject>>;
    fn list(&self, include_archived: bool) -> AppResult<Vec<Subject>>;
    fn update(&self, subject: &Subject, expected_revision: u64) -> AppResult<()>;
}

pub trait TaskRepository: Send + Sync {
    fn insert(&self, task: &StudyTask) -> AppResult<()>;
    fn get(&self, id: Uuid) -> AppResult<Option<StudyTask>>;
    fn list_by_subject(
        &self,
        subject_id: Uuid,
        include_archived: bool,
    ) -> AppResult<Vec<StudyTask>>;
    fn update(&self, task: &StudyTask, expected_revision: u64) -> AppResult<()>;
}

pub trait DailyPlanRepository: Send + Sync {
    fn get_or_create(&self, candidate: &DailyPlan) -> AppResult<DailyPlan>;
    fn get_by_date(
        &self,
        source_device_id: Uuid,
        local_date: NaiveDate,
    ) -> AppResult<Option<DailyPlan>>;
    fn insert_item(&self, item: &DailyPlanItem) -> AppResult<()>;
    fn get_item(&self, id: Uuid) -> AppResult<Option<DailyPlanItem>>;
    fn list_items(&self, daily_plan_id: Uuid) -> AppResult<Vec<DailyPlanItem>>;
    fn update_item(&self, item: &DailyPlanItem, expected_revision: u64) -> AppResult<()>;
}

pub trait SessionRepository: Send + Sync {
    fn create(
        &self,
        session: &Session,
        started_item: Option<(&DailyPlanItem, u64)>,
    ) -> AppResult<()>;
    fn get(&self, id: Uuid) -> AppResult<Option<Session>>;
    fn get_active(&self) -> AppResult<Option<Session>>;
    fn save(&self, session: &Session, expected_revision: u64) -> AppResult<()>;
}

pub trait UnitOfWork: Send + Sync {
    fn verify_integrity(&self) -> AppResult<()>;
}
