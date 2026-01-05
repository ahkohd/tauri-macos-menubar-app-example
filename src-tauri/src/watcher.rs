use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEvent};
use once_cell::sync::Lazy;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::models::{FileChange, FileChangeType, LogEntry, LogSource};
use crate::state::AppState;

// Track last push time per project to debounce rapid file changes
static PUSH_DEBOUNCE: Lazy<Mutex<HashMap<Uuid, Instant>>> = Lazy::new(|| Mutex::new(HashMap::new()));
const PUSH_DEBOUNCE_SECS: u64 = 2;

pub async fn start_watching(
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
    
    state.add_watcher(project_id, debouncer).await;
    state.set_project_watching(project_id, true).await.ok();

    let log = LogEntry::info(
        Some(project_id),
        LogSource::Watcher,
        format!("Started watching: {}", local_path_for_log),
    );
    state.add_log(log.clone()).await;
    app_handle_for_state.emit("log", &log).ok();

    Ok(())
}

pub async fn stop_watching(app_handle: &AppHandle, project_id: Uuid) -> Result<(), String> {
    let state = app_handle.state::<Arc<AppState>>();

    state.stop_watcher(project_id).await;
    state.set_project_watching(project_id, false).await.ok();

    let log = LogEntry::info(
        Some(project_id),
        LogSource::Watcher,
        "Stopped watching".to_string(),
    );
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

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
    let state_arc = state.inner().clone();
    let app_handle_clone = app_handle.clone();

    // Prepare log entry synchronously to avoid moving complex references into async block
    let log = match &change_type {
        FileChangeType::Schema => Some(LogEntry::info(
            Some(project_id),
            LogSource::Schema,
            format!("Schema file changed: {}", get_relative_path(&path_str, base_path)),
        )),
        FileChangeType::EdgeFunction => Some(LogEntry::info(
            Some(project_id),
            LogSource::EdgeFunction,
            format!(
                "Edge function changed: {}",
                get_relative_path(&path_str, base_path)
            ),
        )),
        FileChangeType::Migration => Some(LogEntry::info(
            Some(project_id),
            LogSource::Schema,
            format!(
                "Migration file changed: {}",
                get_relative_path(&path_str, base_path)
            ),
        )),
        FileChangeType::Other => None,
    };

    if let Some(log) = log {
        let log_clone = log.clone();
        let state_for_log = state_arc.clone();
        let app_for_log = app_handle_clone.clone();
        tauri::async_runtime::spawn(async move {
            state_for_log.add_log(log_clone.clone()).await;
            app_for_log.emit("log", &log_clone).ok();
        });
    }

    // Emit the file change event to the frontend
    app_handle.emit("file_change", &file_change).ok();

    // Auto-push for schema changes
    if change_type == FileChangeType::Schema {
        tauri::async_runtime::spawn(async move {
            if let Err(e) = handle_schema_push(state_arc, app_handle_clone, project_id).await {
                eprintln!("Auto-push failed: {}", e);
            }
        });
    }
}

async fn handle_schema_push(
    state: Arc<AppState>,
    app_handle: AppHandle,
    project_id: Uuid,
) -> Result<(), String> {
    // Check debounce - skip if we pushed recently
    {
        let mut debounce = PUSH_DEBOUNCE.lock().await;
        let now = Instant::now();
        if let Some(last_push) = debounce.get(&project_id) {
            if now.duration_since(*last_push) < Duration::from_secs(PUSH_DEBOUNCE_SECS) {
                println!("[DEBUG] Skipping duplicate push for project {} (debounced)", project_id);
                return Ok(());
            }
        }
        debounce.insert(project_id, now);
    }

    // Get project details
    let project = state.get_project(project_id).await.map_err(|e| e.to_string())?;
    
    let project_ref = match &project.supabase_project_ref {
        Some(r) => r.clone(),
        None => {
            let log = LogEntry::warning(
                Some(project_id),
                LogSource::Schema,
                "Auto-push skipped: project not linked to Supabase".to_string(),
            );
            state.add_log(log.clone()).await;
            app_handle.emit("log", &log).ok();
            return Ok(());
        }
    };

    // Get API client
    let api = state.get_api_client().await.map_err(|e| e.to_string())?;

    let log = LogEntry::info(
        Some(project_id),
        LogSource::Schema,
        "Auto-pushing schema changes...".to_string(),
    );
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    // 1. Introspect Remote
    let introspector = crate::introspection::Introspector::new(&api, project_ref.clone());
    let remote_schema = match introspector.introspect().await {
        Ok(schema) => schema,
        Err(e) => {
            let log = LogEntry::error(
                Some(project_id),
                LogSource::Schema,
                format!("Introspection failed: {}", e),
            );
            state.add_log(log.clone()).await;
            app_handle.emit("log", &log).ok();
            return Err(e);
        }
    };

    // 2. Parse Local - try multiple schema paths
    let schema_paths = [
        std::path::Path::new(&project.local_path).join("supabase/schemas/schema.sql"),
        std::path::Path::new(&project.local_path).join("supabase/schema.sql"),
    ];
    
    let schema_path = schema_paths.iter().find(|p| p.exists());
    
    let schema_path = match schema_path {
        Some(p) => p.clone(),
        None => {
            let log = LogEntry::error(
                Some(project_id),
                LogSource::Schema,
                "Schema file not found (checked supabase/schemas/schema.sql and supabase/schema.sql)".to_string(),
            );
            state.add_log(log.clone()).await;
            app_handle.emit("log", &log).ok();
            return Err("Schema file not found".to_string());
        }
    };

    let local_sql = tokio::fs::read_to_string(&schema_path).await.map_err(|e| e.to_string())?;
    let local_schema = match crate::parsing::parse_schema_sql(&local_sql) {
        Ok(schema) => schema,
        Err(e) => {
            let log = LogEntry::error(
                Some(project_id),
                LogSource::Schema,
                format!("Failed to parse schema: {}", e),
            );
            state.add_log(log.clone()).await;
            app_handle.emit("log", &log).ok();
            return Err(e);
        }
    };

    // 3. Diff (Remote -> Local)
    let diff = crate::diff::compute_diff(&remote_schema, &local_schema);

    // 4. Generate Migration SQL
    let migration_sql = crate::generator::generate_sql(&diff, &local_schema);

    if migration_sql.trim().is_empty() {
        let log = LogEntry::success(
            Some(project_id),
            LogSource::Schema,
            "No schema changes detected.".to_string(),
        );
        state.add_log(log.clone()).await;
        app_handle.emit("log", &log).ok();
        return Ok(());
    }

    let log = LogEntry::info(
        Some(project_id),
        LogSource::Schema,
        format!("Applying changes:\n{}", diff.summarize()),
    );
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    // 5. Execute
    let result = api.run_query(&project_ref, &migration_sql, false).await.map_err(|e| e.to_string())?;

    if let Some(err) = result.error {
        let log = LogEntry::error(
            Some(project_id),
            LogSource::Schema,
            format!("Migration failed: {}", err),
        );
        state.add_log(log.clone()).await;
        app_handle.emit("log", &log).ok();
        return Err(err);
    }

    let log = LogEntry::success(
        Some(project_id),
        LogSource::Schema,
        "Schema changes pushed successfully.".to_string(),
    );
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    Ok(())
}

fn classify_file_change(path: &str, base_path: &str) -> FileChangeType {
    let relative = get_relative_path(path, base_path);
    let relative_lower = relative.to_lowercase();

    // Check for schema files (supabase/schema/*.sql, supabase/schemas/*.sql, or schema/*.sql)
    if (relative_lower.contains("/schema") || relative_lower.starts_with("schema"))
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
