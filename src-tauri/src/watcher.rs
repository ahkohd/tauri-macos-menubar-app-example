use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEvent};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::models::{FileChange, FileChangeType, LogEntry, LogSource};
use crate::state::AppState;

pub fn start_watching(
    app_handle: &AppHandle,
    project_id: Uuid,
    local_path: &str,
) -> Result<(), String> {
    let path = Path::new(local_path);
    if !path.exists() {
        return Err(format!("Path does not exist: {}", local_path));
    }

    let app_handle_for_closure = app_handle.clone();
    let app_handle_for_state = app_handle.clone();
    let local_path_for_closure = local_path.to_string();
    let local_path_for_log = local_path.to_string();

    // Create debouncer with 500ms debounce time
    let mut debouncer = new_debouncer(
        Duration::from_millis(500),
        move |result: Result<Vec<DebouncedEvent>, notify::Error>| {
            match result {
                Ok(events) => {
                    for event in events {
                        handle_file_event(&app_handle_for_closure, project_id, &local_path_for_closure, event);
                    }
                }
                Err(e) => {
                    eprintln!("Watch error: {:?}", e);
                }
            }
        },
    )
    .map_err(|e| format!("Failed to create watcher: {}", e))?;

    // Watch the directory recursively
    debouncer
        .watcher()
        .watch(path, RecursiveMode::Recursive)
        .map_err(|e| format!("Failed to watch path: {}", e))?;

    // Store the watcher handle
    let state = app_handle_for_state.state::<Arc<AppState>>();
    tauri::async_runtime::block_on(async {
        state.add_watcher(project_id, debouncer).await;
        state.set_project_watching(project_id, true).await.ok();

        let log = LogEntry::info(
            Some(project_id),
            LogSource::Watcher,
            format!("Started watching: {}", local_path_for_log),
        );
        state.add_log(log.clone()).await;
        app_handle_for_state.emit("log", &log).ok();
    });

    Ok(())
}

pub fn stop_watching(app_handle: &AppHandle, project_id: Uuid) -> Result<(), String> {
    let state = app_handle.state::<Arc<AppState>>();

    tauri::async_runtime::block_on(async {
        state.stop_watcher(project_id).await;
        state.set_project_watching(project_id, false).await.ok();

        let log = LogEntry::info(
            Some(project_id),
            LogSource::Watcher,
            "Stopped watching".to_string(),
        );
        state.add_log(log.clone()).await;
        app_handle.emit("log", &log).ok();
    });

    Ok(())
}

fn handle_file_event(
    app_handle: &AppHandle,
    project_id: Uuid,
    base_path: &str,
    event: DebouncedEvent,
) {
    let path = event.path;
    let path_str = path.to_string_lossy().to_string();

    // Determine the type of change based on the file path
    let change_type = classify_file_change(&path_str, base_path);

    // Skip if it's not a file we care about
    if change_type == FileChangeType::Other {
        return;
    }

    let file_change = FileChange::new(path_str.clone(), change_type.clone(), project_id);

    let state = app_handle.state::<Arc<AppState>>();

    tauri::async_runtime::block_on(async {
        let log = match &change_type {
            FileChangeType::Schema => LogEntry::info(
                Some(project_id),
                LogSource::Schema,
                format!("Schema file changed: {}", get_relative_path(&path_str, base_path)),
            ),
            FileChangeType::EdgeFunction => LogEntry::info(
                Some(project_id),
                LogSource::EdgeFunction,
                format!(
                    "Edge function changed: {}",
                    get_relative_path(&path_str, base_path)
                ),
            ),
            FileChangeType::Migration => LogEntry::info(
                Some(project_id),
                LogSource::Schema,
                format!(
                    "Migration file changed: {}",
                    get_relative_path(&path_str, base_path)
                ),
            ),
            FileChangeType::Other => return,
        };

        state.add_log(log.clone()).await;
        app_handle.emit("log", &log).ok();
    });

    // Emit the file change event to the frontend
    app_handle.emit("file_change", &file_change).ok();
}

fn classify_file_change(path: &str, base_path: &str) -> FileChangeType {
    let relative = get_relative_path(path, base_path);
    let relative_lower = relative.to_lowercase();

    // Check for schema files (supabase/schema/*.sql or schema/*.sql)
    if (relative_lower.contains("/schema/") || relative_lower.starts_with("schema/"))
        && relative_lower.ends_with(".sql")
    {
        return FileChangeType::Schema;
    }

    // Check for edge functions (supabase/functions/* or functions/*)
    if (relative_lower.contains("/functions/") || relative_lower.starts_with("functions/"))
        && (relative_lower.ends_with(".ts") || relative_lower.ends_with(".js"))
    {
        return FileChangeType::EdgeFunction;
    }

    // Check for migrations (supabase/migrations/*.sql or migrations/*.sql)
    if (relative_lower.contains("/migrations/") || relative_lower.starts_with("migrations/"))
        && relative_lower.ends_with(".sql")
    {
        return FileChangeType::Migration;
    }

    FileChangeType::Other
}

fn get_relative_path(path: &str, base_path: &str) -> String {
    path.strip_prefix(base_path)
        .unwrap_or(path)
        .trim_start_matches('/')
        .to_string()
}
