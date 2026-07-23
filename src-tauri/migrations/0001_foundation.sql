PRAGMA foreign_keys = ON;

CREATE TABLE devices (
    id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL CHECK (length(trim(display_name)) > 0),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    deleted_at_utc TEXT
) STRICT;

CREATE TABLE subjects (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    sort_order INTEGER NOT NULL DEFAULT 0,
    archived_at_utc TEXT,
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    source_device_id TEXT NOT NULL REFERENCES devices(id),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    deleted_at_utc TEXT
) STRICT;

CREATE TABLE tasks (
    id TEXT PRIMARY KEY NOT NULL,
    subject_id TEXT NOT NULL REFERENCES subjects(id),
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    status TEXT NOT NULL CHECK (status IN ('open', 'completed', 'archived')),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    source_device_id TEXT NOT NULL REFERENCES devices(id),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    deleted_at_utc TEXT
) STRICT;

CREATE TABLE daily_plans (
    id TEXT PRIMARY KEY NOT NULL,
    local_date TEXT NOT NULL,
    time_zone_id TEXT NOT NULL,
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    source_device_id TEXT NOT NULL REFERENCES devices(id),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    deleted_at_utc TEXT,
    UNIQUE (local_date, source_device_id)
) STRICT;

CREATE TABLE daily_plan_items (
    id TEXT PRIMARY KEY NOT NULL,
    daily_plan_id TEXT NOT NULL REFERENCES daily_plans(id),
    subject_id TEXT NOT NULL REFERENCES subjects(id),
    task_id TEXT REFERENCES tasks(id),
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    status TEXT NOT NULL CHECK (status IN ('planned', 'in_progress', 'completed', 'incomplete', 'deferred')),
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    source_device_id TEXT NOT NULL REFERENCES devices(id),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    deleted_at_utc TEXT
) STRICT;

CREATE TABLE sessions (
    id TEXT PRIMARY KEY NOT NULL,
    subject_id TEXT NOT NULL REFERENCES subjects(id),
    task_id TEXT REFERENCES tasks(id),
    daily_plan_item_id TEXT REFERENCES daily_plan_items(id),
    timer_mode TEXT NOT NULL CHECK (timer_mode IN ('stopwatch', 'countdown', 'pomodoro')),
    state TEXT NOT NULL CHECK (state IN ('focus', 'break', 'paused', 'ended')),
    started_at_utc TEXT NOT NULL,
    ended_at_utc TEXT,
    time_zone_id TEXT NOT NULL,
    timer_config_json TEXT NOT NULL DEFAULT '{}',
    recovery_checkpoint_utc TEXT NOT NULL,
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    source_device_id TEXT NOT NULL REFERENCES devices(id),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    deleted_at_utc TEXT,
    CHECK (ended_at_utc IS NULL OR ended_at_utc >= started_at_utc)
) STRICT;

CREATE UNIQUE INDEX only_one_active_session
ON sessions ((1))
WHERE ended_at_utc IS NULL AND deleted_at_utc IS NULL;

CREATE TABLE session_segments (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('focus', 'idle', 'break', 'paused', 'pending')),
    started_at_utc TEXT NOT NULL,
    ended_at_utc TEXT,
    pending_reason TEXT,
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    source_device_id TEXT NOT NULL REFERENCES devices(id),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    deleted_at_utc TEXT,
    CHECK (ended_at_utc IS NULL OR ended_at_utc >= started_at_utc)
) STRICT;

CREATE INDEX session_segments_by_session_time
ON session_segments (session_id, started_at_utc);

CREATE TABLE activity_targets (
    id TEXT PRIMARY KEY NOT NULL,
    stable_key TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('application', 'unknown')),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    source_device_id TEXT NOT NULL REFERENCES devices(id),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    deleted_at_utc TEXT
) STRICT;

CREATE TABLE activity_intervals (
    id TEXT PRIMARY KEY NOT NULL,
    target_id TEXT REFERENCES activity_targets(id),
    session_id TEXT REFERENCES sessions(id),
    state_context TEXT NOT NULL CHECK (state_context IN ('idle', 'focus', 'break', 'paused')),
    started_at_utc TEXT NOT NULL,
    ended_at_utc TEXT,
    window_title TEXT,
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    source_device_id TEXT NOT NULL REFERENCES devices(id),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    deleted_at_utc TEXT,
    CHECK (ended_at_utc IS NULL OR ended_at_utc >= started_at_utc)
) STRICT;

CREATE INDEX activity_intervals_by_time
ON activity_intervals (started_at_utc, ended_at_utc);

CREATE TABLE rules (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    action TEXT NOT NULL CHECK (action IN ('allow', 'deny')),
    matcher_kind TEXT NOT NULL CHECK (matcher_kind IN ('application', 'title_exact', 'title_contains')),
    matcher_value TEXT NOT NULL,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('global', 'device', 'subject', 'task', 'session')),
    scope_id TEXT,
    mode TEXT NOT NULL CHECK (mode IN ('blacklist', 'whitelist')),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    source_device_id TEXT NOT NULL REFERENCES devices(id),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    deleted_at_utc TEXT
) STRICT;

CREATE TABLE temporary_allowances (
    id TEXT PRIMARY KEY NOT NULL,
    rule_id TEXT REFERENCES rules(id),
    target_id TEXT NOT NULL REFERENCES activity_targets(id),
    session_id TEXT REFERENCES sessions(id),
    allowed_from_utc TEXT NOT NULL,
    allowed_until_utc TEXT,
    allowance_kind TEXT NOT NULL CHECK (allowance_kind IN ('once', 'duration', 'session')),
    created_at_utc TEXT NOT NULL,
    source_device_id TEXT NOT NULL REFERENCES devices(id)
) STRICT;

CREATE TABLE distraction_events (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    target_id TEXT NOT NULL REFERENCES activity_targets(id),
    rule_snapshot_json TEXT NOT NULL,
    started_at_utc TEXT NOT NULL,
    ended_at_utc TEXT,
    decision TEXT CHECK (decision IN ('continue', 'cancel')),
    excluded_from_statistics INTEGER NOT NULL DEFAULT 0 CHECK (excluded_from_statistics IN (0, 1)),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    source_device_id TEXT NOT NULL REFERENCES devices(id),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    deleted_at_utc TEXT
) STRICT;

CREATE TABLE privacy_schedules (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    schedule_kind TEXT NOT NULL CHECK (schedule_kind IN ('weekly', 'one_shot')),
    schedule_json TEXT NOT NULL,
    time_zone_id TEXT NOT NULL,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    source_device_id TEXT NOT NULL REFERENCES devices(id),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    deleted_at_utc TEXT
) STRICT;

CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL,
    schema_version INTEGER NOT NULL CHECK (schema_version >= 0),
    updated_at_utc TEXT NOT NULL
) STRICT;

CREATE TABLE audit_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    entity_kind TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    previous_revision INTEGER NOT NULL,
    new_revision INTEGER NOT NULL,
    change_summary_json TEXT NOT NULL,
    changed_at_utc TEXT NOT NULL,
    source_device_id TEXT NOT NULL REFERENCES devices(id)
) STRICT;

CREATE TABLE backup_metadata (
    id TEXT PRIMARY KEY NOT NULL,
    format_version INTEGER NOT NULL CHECK (format_version > 0),
    created_at_utc TEXT NOT NULL,
    destination_kind TEXT NOT NULL,
    checksum_sha256 TEXT,
    encrypted INTEGER NOT NULL CHECK (encrypted IN (0, 1)),
    verified_at_utc TEXT,
    status TEXT NOT NULL CHECK (status IN ('creating', 'valid', 'failed'))
) STRICT;
