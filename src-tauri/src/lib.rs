pub mod adapters;
pub mod application;
pub mod domain;
pub mod error;
pub mod logging;
pub mod ports;
pub mod runtime;

use std::{fs, sync::Arc};

use adapters::persistence::SqliteStore;
use application::settings::SettingsService;
use domain::settings::SETTINGS_SCHEMA_VERSION;
use error::{AppError, IpcError};
use ports::repository::UnitOfWork;
use runtime::{HeartbeatResource, RuntimeCoordinator, RuntimeState};
use serde::Serialize;
use tauri::{Manager, State};

struct AppState {
    store: Arc<SqliteStore>,
    coordinator: RuntimeCoordinator,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FoundationStatus {
    app_version: &'static str,
    database_schema_version: u32,
    settings_schema_version: u32,
    runtime_state: &'static str,
}

#[tauri::command]
fn foundation_status(state: State<'_, AppState>) -> Result<FoundationStatus, IpcError> {
    let database_schema_version = state.store.schema_version().map_err(IpcError::from)?;
    let runtime_state = match state.coordinator.state().map_err(IpcError::from)? {
        RuntimeState::Running => "running",
        RuntimeState::Stopped => "stopped",
    };
    Ok(FoundationStatus {
        app_version: env!("CARGO_PKG_VERSION"),
        database_schema_version,
        settings_schema_version: SETTINGS_SCHEMA_VERSION,
        runtime_state,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::initialize();
    let app = tauri::Builder::default()
        .setup(|app| {
            let app_data = app
                .path()
                .app_data_dir()
                .map_err(|_| AppError::Initialization)?;
            fs::create_dir_all(&app_data).map_err(|_| AppError::Initialization)?;

            let store = Arc::new(SqliteStore::open(app_data.join("studytracker.sqlite3"))?);
            store.verify_integrity()?;
            let settings_repository: Arc<dyn ports::repository::SettingsRepository> = store.clone();
            SettingsService::new(settings_repository).load_or_create_defaults()?;

            let coordinator = RuntimeCoordinator::new(vec![Arc::new(HeartbeatResource::default())]);
            coordinator.start()?;
            app.manage(AppState { store, coordinator });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![foundation_status])
        .build(tauri::generate_context!())
        .expect("無法建立 StudyTracker 應用程式");

    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            let state = app_handle.state::<AppState>();
            let _ = state.coordinator.stop();
        }
    });
}
