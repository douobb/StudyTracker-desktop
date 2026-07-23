use std::collections::HashSet;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerMode {
    Stopwatch,
    Countdown,
    Pomodoro,
}

impl TimerMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stopwatch => "stopwatch",
            Self::Countdown => "countdown",
            Self::Pomodoro => "pomodoro",
        }
    }

    pub fn from_persisted(value: &str) -> AppResult<Self> {
        match value {
            "stopwatch" => Ok(Self::Stopwatch),
            "countdown" => Ok(Self::Countdown),
            "pomodoro" => Ok(Self::Pomodoro),
            _ => Err(AppError::InvalidSession("timer_mode")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Focus,
    Break,
    Paused,
    Ended,
}

impl SessionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Focus => "focus",
            Self::Break => "break",
            Self::Paused => "paused",
            Self::Ended => "ended",
        }
    }

    pub fn from_persisted(value: &str) -> AppResult<Self> {
        match value {
            "focus" => Ok(Self::Focus),
            "break" => Ok(Self::Break),
            "paused" => Ok(Self::Paused),
            "ended" => Ok(Self::Ended),
            _ => Err(AppError::InvalidSession("state")),
        }
    }

    fn segment_kind(self) -> Option<SegmentKind> {
        match self {
            Self::Focus => Some(SegmentKind::Focus),
            Self::Break => Some(SegmentKind::Break),
            Self::Paused => Some(SegmentKind::Paused),
            Self::Ended => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    Focus,
    Idle,
    Break,
    Paused,
    Pending,
}

impl SegmentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Focus => "focus",
            Self::Idle => "idle",
            Self::Break => "break",
            Self::Paused => "paused",
            Self::Pending => "pending",
        }
    }

    pub fn from_persisted(value: &str) -> AppResult<Self> {
        match value {
            "focus" => Ok(Self::Focus),
            "idle" => Ok(Self::Idle),
            "break" => Ok(Self::Break),
            "paused" => Ok(Self::Paused),
            "pending" => Ok(Self::Pending),
            _ => Err(AppError::InvalidSession("segment_kind")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSegment {
    id: Uuid,
    session_id: Uuid,
    kind: SegmentKind,
    started_at_utc: DateTime<Utc>,
    ended_at_utc: Option<DateTime<Utc>>,
    pending_reason: Option<String>,
    created_at_utc: DateTime<Utc>,
    updated_at_utc: DateTime<Utc>,
    source_device_id: Uuid,
    revision: u64,
    deleted_at_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSegmentSnapshot {
    pub id: Uuid,
    pub session_id: Uuid,
    pub kind: SegmentKind,
    pub started_at_utc: DateTime<Utc>,
    pub ended_at_utc: Option<DateTime<Utc>>,
    pub pending_reason: Option<String>,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    pub source_device_id: Uuid,
    pub revision: u64,
    pub deleted_at_utc: Option<DateTime<Utc>>,
}

impl SessionSegment {
    fn open(
        id: Uuid,
        session_id: Uuid,
        kind: SegmentKind,
        now: DateTime<Utc>,
        source_device_id: Uuid,
    ) -> Self {
        Self {
            id,
            session_id,
            kind,
            started_at_utc: now,
            ended_at_utc: None,
            pending_reason: None,
            created_at_utc: now,
            updated_at_utc: now,
            source_device_id,
            revision: 1,
            deleted_at_utc: None,
        }
    }

    pub fn restore(snapshot: SessionSegmentSnapshot) -> AppResult<Self> {
        if snapshot.revision == 0
            || snapshot.updated_at_utc < snapshot.created_at_utc
            || snapshot
                .ended_at_utc
                .is_some_and(|ended_at| ended_at < snapshot.started_at_utc)
            || snapshot
                .deleted_at_utc
                .is_some_and(|deleted_at| deleted_at < snapshot.created_at_utc)
            || (snapshot.kind != SegmentKind::Pending && snapshot.pending_reason.is_some())
        {
            return Err(AppError::InvalidSession("segment"));
        }
        Ok(Self {
            id: snapshot.id,
            session_id: snapshot.session_id,
            kind: snapshot.kind,
            started_at_utc: snapshot.started_at_utc,
            ended_at_utc: snapshot.ended_at_utc,
            pending_reason: snapshot.pending_reason,
            created_at_utc: snapshot.created_at_utc,
            updated_at_utc: snapshot.updated_at_utc,
            source_device_id: snapshot.source_device_id,
            revision: snapshot.revision,
            deleted_at_utc: snapshot.deleted_at_utc,
        })
    }

    fn close(&mut self, now: DateTime<Utc>) -> AppResult<()> {
        if self.ended_at_utc.is_some() || now < self.started_at_utc || now < self.updated_at_utc {
            return Err(AppError::InvalidSessionTransition);
        }
        let next_revision = self
            .revision
            .checked_add(1)
            .ok_or(AppError::InvalidSession("segment_revision"))?;
        self.ended_at_utc = Some(now);
        self.updated_at_utc = now;
        self.revision = next_revision;
        Ok(())
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    pub fn kind(&self) -> SegmentKind {
        self.kind
    }

    pub fn started_at_utc(&self) -> DateTime<Utc> {
        self.started_at_utc
    }

    pub fn ended_at_utc(&self) -> Option<DateTime<Utc>> {
        self.ended_at_utc
    }

    pub fn pending_reason(&self) -> Option<&str> {
        self.pending_reason.as_deref()
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    id: Uuid,
    subject_id: Uuid,
    task_id: Option<Uuid>,
    daily_plan_item_id: Option<Uuid>,
    timer_mode: TimerMode,
    state: SessionState,
    started_at_utc: DateTime<Utc>,
    ended_at_utc: Option<DateTime<Utc>>,
    time_zone: Tz,
    timer_config_json: String,
    recovery_checkpoint_utc: DateTime<Utc>,
    created_at_utc: DateTime<Utc>,
    updated_at_utc: DateTime<Utc>,
    source_device_id: Uuid,
    revision: u64,
    deleted_at_utc: Option<DateTime<Utc>>,
    segments: Vec<SessionSegment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionIds {
    pub id: Uuid,
    pub first_segment_id: Uuid,
    pub subject_id: Uuid,
    pub task_id: Option<Uuid>,
    pub daily_plan_item_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub id: Uuid,
    pub subject_id: Uuid,
    pub task_id: Option<Uuid>,
    pub daily_plan_item_id: Option<Uuid>,
    pub timer_mode: TimerMode,
    pub state: SessionState,
    pub started_at_utc: DateTime<Utc>,
    pub ended_at_utc: Option<DateTime<Utc>>,
    pub time_zone: Tz,
    pub timer_config_json: String,
    pub recovery_checkpoint_utc: DateTime<Utc>,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    pub source_device_id: Uuid,
    pub revision: u64,
    pub deleted_at_utc: Option<DateTime<Utc>>,
}

impl Session {
    pub fn start(
        ids: SessionIds,
        timer_mode: TimerMode,
        time_zone: Tz,
        timer_config_json: impl Into<String>,
        now: DateTime<Utc>,
        source_device_id: Uuid,
    ) -> AppResult<Self> {
        let timer_config_json = timer_config_json.into();
        validate_timer_config(&timer_config_json)?;
        Ok(Self {
            id: ids.id,
            subject_id: ids.subject_id,
            task_id: ids.task_id,
            daily_plan_item_id: ids.daily_plan_item_id,
            timer_mode,
            state: SessionState::Focus,
            started_at_utc: now,
            ended_at_utc: None,
            time_zone,
            timer_config_json,
            recovery_checkpoint_utc: now,
            created_at_utc: now,
            updated_at_utc: now,
            source_device_id,
            revision: 1,
            deleted_at_utc: None,
            segments: vec![SessionSegment::open(
                ids.first_segment_id,
                ids.id,
                SegmentKind::Focus,
                now,
                source_device_id,
            )],
        })
    }

    pub fn restore(snapshot: SessionSnapshot, segments: Vec<SessionSegment>) -> AppResult<Self> {
        validate_timer_config(&snapshot.timer_config_json)?;
        let session = Self {
            id: snapshot.id,
            subject_id: snapshot.subject_id,
            task_id: snapshot.task_id,
            daily_plan_item_id: snapshot.daily_plan_item_id,
            timer_mode: snapshot.timer_mode,
            state: snapshot.state,
            started_at_utc: snapshot.started_at_utc,
            ended_at_utc: snapshot.ended_at_utc,
            time_zone: snapshot.time_zone,
            timer_config_json: snapshot.timer_config_json,
            recovery_checkpoint_utc: snapshot.recovery_checkpoint_utc,
            created_at_utc: snapshot.created_at_utc,
            updated_at_utc: snapshot.updated_at_utc,
            source_device_id: snapshot.source_device_id,
            revision: snapshot.revision,
            deleted_at_utc: snapshot.deleted_at_utc,
            segments,
        };
        session.validate()?;
        Ok(session)
    }

    pub fn pause(&mut self, next_segment_id: Uuid, now: DateTime<Utc>) -> AppResult<()> {
        if !matches!(self.state, SessionState::Focus | SessionState::Break) {
            return Err(AppError::InvalidSessionTransition);
        }
        self.apply_change(|candidate| {
            candidate.transition(SessionState::Paused, next_segment_id, now)
        })
    }

    pub fn resume(&mut self, next_segment_id: Uuid, now: DateTime<Utc>) -> AppResult<()> {
        if self.state != SessionState::Paused {
            return Err(AppError::InvalidSessionTransition);
        }
        let resume_state = self
            .segments
            .iter()
            .rev()
            .skip(1)
            .find_map(|segment| match segment.kind() {
                SegmentKind::Focus => Some(SessionState::Focus),
                SegmentKind::Break => Some(SessionState::Break),
                _ => None,
            })
            .ok_or(AppError::InvalidSessionTransition)?;
        self.apply_change(|candidate| candidate.transition(resume_state, next_segment_id, now))
    }

    pub fn start_break(&mut self, next_segment_id: Uuid, now: DateTime<Utc>) -> AppResult<()> {
        if self.state != SessionState::Focus {
            return Err(AppError::InvalidSessionTransition);
        }
        self.apply_change(|candidate| {
            candidate.transition(SessionState::Break, next_segment_id, now)
        })
    }

    pub fn end_break(&mut self, next_segment_id: Uuid, now: DateTime<Utc>) -> AppResult<()> {
        if self.state != SessionState::Break {
            return Err(AppError::InvalidSessionTransition);
        }
        self.apply_change(|candidate| {
            candidate.transition(SessionState::Focus, next_segment_id, now)
        })
    }

    pub fn end(&mut self, now: DateTime<Utc>) -> AppResult<()> {
        if self.state == SessionState::Ended {
            return Err(AppError::InvalidSessionTransition);
        }
        self.apply_change(|candidate| {
            candidate.ensure_current_segment_matches_state()?;
            candidate.current_segment_mut()?.close(now)?;
            candidate.state = SessionState::Ended;
            candidate.ended_at_utc = Some(now);
            candidate.mark_changed(now)?;
            candidate.validate()
        })
    }

    pub fn ensure_revision(&self, expected_revision: u64) -> AppResult<()> {
        if self.revision != expected_revision {
            return Err(AppError::RevisionConflict);
        }
        Ok(())
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.revision == 0
            || self.updated_at_utc < self.created_at_utc
            || self.started_at_utc != self.created_at_utc
            || self.recovery_checkpoint_utc < self.started_at_utc
            || self.recovery_checkpoint_utc > self.updated_at_utc
            || self
                .ended_at_utc
                .is_some_and(|ended_at| ended_at < self.started_at_utc)
            || self
                .deleted_at_utc
                .is_some_and(|deleted_at| deleted_at < self.created_at_utc)
            || self.segments.is_empty()
        {
            return Err(AppError::InvalidSession("metadata"));
        }
        let mut previous_end = None;
        let mut segment_ids = HashSet::with_capacity(self.segments.len());
        for (index, segment) in self.segments.iter().enumerate() {
            if !segment_ids.insert(segment.id())
                || segment.session_id() != self.id
                || segment.deleted_at_utc().is_some()
            {
                return Err(AppError::InvalidSession("segment_reference"));
            }
            if index == 0 && segment.started_at_utc() != self.started_at_utc {
                return Err(AppError::InvalidSession("segment_start"));
            }
            if let Some(end) = previous_end
                && segment.started_at_utc() != end
            {
                return Err(AppError::InvalidSession("segment_gap_or_overlap"));
            }
            if index + 1 < self.segments.len() && segment.ended_at_utc().is_none() {
                return Err(AppError::InvalidSession("open_segment"));
            }
            previous_end = segment.ended_at_utc();
        }
        let current = self
            .segments
            .last()
            .ok_or(AppError::InvalidSession("segments"))?;
        match self.state {
            SessionState::Ended => {
                if self.ended_at_utc.is_none() || current.ended_at_utc() != self.ended_at_utc {
                    return Err(AppError::InvalidSession("ended_state"));
                }
            }
            active_state => {
                if self.ended_at_utc.is_some()
                    || current.ended_at_utc().is_some()
                    || (current.kind() != SegmentKind::Pending
                        && Some(current.kind()) != active_state.segment_kind())
                {
                    return Err(AppError::InvalidSession("active_state"));
                }
            }
        }
        Ok(())
    }

    fn transition(
        &mut self,
        next_state: SessionState,
        next_segment_id: Uuid,
        now: DateTime<Utc>,
    ) -> AppResult<()> {
        self.ensure_current_segment_matches_state()?;
        self.current_segment_mut()?.close(now)?;
        self.segments.push(SessionSegment::open(
            next_segment_id,
            self.id,
            next_state
                .segment_kind()
                .ok_or(AppError::InvalidSessionTransition)?,
            now,
            self.source_device_id,
        ));
        self.state = next_state;
        self.mark_changed(now)?;
        self.validate()
    }

    fn apply_change(
        &mut self,
        operation: impl FnOnce(&mut Session) -> AppResult<()>,
    ) -> AppResult<()> {
        let mut candidate = self.clone();
        operation(&mut candidate)?;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    fn ensure_current_segment_matches_state(&self) -> AppResult<()> {
        let current = self
            .segments
            .last()
            .ok_or(AppError::InvalidSessionTransition)?;
        if current.ended_at_utc().is_some() || Some(current.kind()) != self.state.segment_kind() {
            return Err(AppError::InvalidSessionTransition);
        }
        Ok(())
    }

    fn current_segment_mut(&mut self) -> AppResult<&mut SessionSegment> {
        self.segments
            .last_mut()
            .ok_or(AppError::InvalidSessionTransition)
    }

    fn mark_changed(&mut self, now: DateTime<Utc>) -> AppResult<()> {
        if now < self.updated_at_utc || now < self.started_at_utc {
            return Err(AppError::InvalidSessionTransition);
        }
        let next_revision = self
            .revision
            .checked_add(1)
            .ok_or(AppError::InvalidSession("revision"))?;
        self.updated_at_utc = now;
        self.recovery_checkpoint_utc = now;
        self.revision = next_revision;
        Ok(())
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn subject_id(&self) -> Uuid {
        self.subject_id
    }

    pub fn task_id(&self) -> Option<Uuid> {
        self.task_id
    }

    pub fn daily_plan_item_id(&self) -> Option<Uuid> {
        self.daily_plan_item_id
    }

    pub fn timer_mode(&self) -> TimerMode {
        self.timer_mode
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn started_at_utc(&self) -> DateTime<Utc> {
        self.started_at_utc
    }

    pub fn ended_at_utc(&self) -> Option<DateTime<Utc>> {
        self.ended_at_utc
    }

    pub fn time_zone(&self) -> Tz {
        self.time_zone
    }

    pub fn timer_config_json(&self) -> &str {
        &self.timer_config_json
    }

    pub fn recovery_checkpoint_utc(&self) -> DateTime<Utc> {
        self.recovery_checkpoint_utc
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

    pub fn segments(&self) -> &[SessionSegment] {
        &self.segments
    }
}

fn validate_timer_config(value: &str) -> AppResult<()> {
    let parsed: serde_json::Value =
        serde_json::from_str(value).map_err(|_| AppError::InvalidSession("timer_config"))?;
    if !parsed.is_object() {
        return Err(AppError::InvalidSession("timer_config"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use chrono_tz::Asia::Taipei;
    use uuid::Uuid;

    use super::{
        SegmentKind, Session, SessionIds, SessionSegment, SessionSegmentSnapshot, SessionSnapshot,
        SessionState, TimerMode,
    };
    use crate::error::AppError;

    fn session() -> Session {
        Session::start(
            SessionIds {
                id: Uuid::now_v7(),
                first_segment_id: Uuid::now_v7(),
                subject_id: Uuid::now_v7(),
                task_id: None,
                daily_plan_item_id: None,
            },
            TimerMode::Stopwatch,
            Taipei,
            "{}",
            Utc.with_ymd_and_hms(2026, 7, 23, 8, 0, 0).unwrap(),
            Uuid::now_v7(),
        )
        .unwrap()
    }

    #[test]
    fn 開始時原子建立專注區段() {
        let session = session();
        assert_eq!(session.state(), SessionState::Focus);
        assert_eq!(session.segments().len(), 1);
        assert_eq!(session.segments()[0].kind(), SegmentKind::Focus);
        assert!(session.segments()[0].ended_at_utc().is_none());
        assert!(session.validate().is_ok());
    }

    #[test]
    fn 完整狀態轉移保持連續且不重疊() {
        let mut session = session();
        let start = session.started_at_utc();
        session
            .start_break(Uuid::now_v7(), start + Duration::minutes(25))
            .unwrap();
        session
            .pause(Uuid::now_v7(), start + Duration::minutes(27))
            .unwrap();
        session
            .resume(Uuid::now_v7(), start + Duration::minutes(28))
            .unwrap();
        assert_eq!(session.state(), SessionState::Break);
        session
            .end_break(Uuid::now_v7(), start + Duration::minutes(30))
            .unwrap();
        session.end(start + Duration::minutes(40)).unwrap();
        assert_eq!(session.state(), SessionState::Ended);
        assert_eq!(session.segments().len(), 5);
        for pair in session.segments().windows(2) {
            assert_eq!(pair[0].ended_at_utc(), Some(pair[1].started_at_utc()));
        }
        assert!(session.validate().is_ok());
    }

    #[test]
    fn 非法轉移與時間倒退不修改狀態() {
        let mut session = session();
        let original = session.clone();
        assert!(matches!(
            session.resume(Uuid::now_v7(), session.started_at_utc()),
            Err(AppError::InvalidSessionTransition)
        ));
        assert_eq!(session, original);
        assert!(matches!(
            session.pause(
                Uuid::now_v7(),
                session.started_at_utc() - Duration::seconds(1)
            ),
            Err(AppError::InvalidSessionTransition)
        ));
        assert_eq!(session, original);
    }

    #[test]
    fn 還原時拒絕重疊區段() {
        let session = session();
        let start = session.started_at_utc();
        let first = SessionSegment::restore(SessionSegmentSnapshot {
            id: Uuid::now_v7(),
            session_id: session.id(),
            kind: SegmentKind::Focus,
            started_at_utc: start,
            ended_at_utc: Some(start + Duration::minutes(10)),
            pending_reason: None,
            created_at_utc: start,
            updated_at_utc: start + Duration::minutes(10),
            source_device_id: session.source_device_id(),
            revision: 2,
            deleted_at_utc: None,
        })
        .unwrap();
        let overlapping = SessionSegment::restore(SessionSegmentSnapshot {
            id: Uuid::now_v7(),
            session_id: session.id(),
            kind: SegmentKind::Break,
            started_at_utc: start + Duration::minutes(9),
            ended_at_utc: None,
            pending_reason: None,
            created_at_utc: start + Duration::minutes(9),
            updated_at_utc: start + Duration::minutes(9),
            source_device_id: session.source_device_id(),
            revision: 1,
            deleted_at_utc: None,
        })
        .unwrap();
        let restored = Session::restore(
            SessionSnapshot {
                id: session.id(),
                subject_id: session.subject_id(),
                task_id: None,
                daily_plan_item_id: None,
                timer_mode: TimerMode::Stopwatch,
                state: SessionState::Break,
                started_at_utc: start,
                ended_at_utc: None,
                time_zone: Taipei,
                timer_config_json: "{}".to_owned(),
                recovery_checkpoint_utc: start + Duration::minutes(9),
                created_at_utc: start,
                updated_at_utc: start + Duration::minutes(9),
                source_device_id: session.source_device_id(),
                revision: 2,
                deleted_at_utc: None,
            },
            vec![first, overlapping],
        );
        assert!(matches!(
            restored,
            Err(AppError::InvalidSession("segment_gap_or_overlap"))
        ));
    }
}
