use std::path::Path;
use std::sync::{Arc, Once};

use tauri::{Emitter, Manager};
use tauri_nspanel::ManagerExt;
use uuid::Uuid;

use crate::fns::{
    setup_menubar_panel_listeners, swizzle_to_menubar_panel, update_menubar_appearance,
};
use crate::models::{LogEntry, LogSource, Project, RemoteProject};
use crate::state::AppState;
use crate::watcher;

static INIT: Once = Once::new();

#[tauri::command]
pub fn init(app_handle: tauri::AppHandle) {
    INIT.call_once(|| {
        swizzle_to_menubar_panel(&app_handle);

        update_menubar_appearance(&app_handle);

        setup_menubar_panel_listeners(&app_handle);
    });
}

#[tauri::command]
pub fn show_menubar_panel(app_handle: tauri::AppHandle) {
    let panel = app_handle.get_webview_panel("main").unwrap();

    panel.show();
}

// Access token commands

#[tauri::command]
pub async fn set_access_token(
    app_handle: tauri::AppHandle,
    token: String,
) -> Result<(), String> {
    let state = app_handle.state::<Arc<AppState>>();
    state.set_access_token(token).await.map_err(|e| e.to_string())?;

    let log = LogEntry::success(None, LogSource::System, "Access token saved".to_string());
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    Ok(())
}

#[tauri::command]
pub async fn has_access_token(app_handle: tauri::AppHandle) -> Result<bool, String> {
    let state = app_handle.state::<Arc<AppState>>();
    Ok(state.has_access_token().await)
}

#[tauri::command]
pub async fn clear_access_token(app_handle: tauri::AppHandle) -> Result<(), String> {
    let state = app_handle.state::<Arc<AppState>>();
    state.clear_access_token().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn validate_access_token(app_handle: tauri::AppHandle) -> Result<bool, String> {
    let state = app_handle.state::<Arc<AppState>>();
    let api = state.get_api_client().await.map_err(|e| e.to_string())?;

    match api.list_projects().await {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

// Remote Supabase project commands

#[tauri::command]
pub async fn list_remote_projects(
    app_handle: tauri::AppHandle,
) -> Result<Vec<RemoteProject>, String> {
    let state = app_handle.state::<Arc<AppState>>();
    let api = state.get_api_client().await.map_err(|e| e.to_string())?;

    let projects = api.list_projects().await.map_err(|e| e.to_string())?;

    Ok(projects
        .into_iter()
        .map(|p| RemoteProject {
            id: p.id,
            name: p.name,
            organization_id: p.organization_id,
            region: p.region,
            created_at: p.created_at,
        })
        .collect())
}

// Project commands

#[tauri::command]
pub async fn create_project(
    app_handle: tauri::AppHandle,
    name: String,
    local_path: String,
    supabase_project_id: Option<String>,
    supabase_project_ref: Option<String>,
) -> Result<Project, String> {
    let state = app_handle.state::<Arc<AppState>>();

    let project = if let (Some(project_id), Some(project_ref)) =
        (supabase_project_id, supabase_project_ref)
    {
        Project::with_remote(name, local_path, project_id, project_ref)
    } else {
        Project::new(name, local_path)
    };

    let result = state
        .add_project(project)
        .await
        .map_err(|e| e.to_string())?;

    let log = LogEntry::success(
        Some(result.id),
        LogSource::System,
        format!("Created project: {}", result.name),
    );
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    Ok(result)
}

#[tauri::command]
pub async fn get_projects(app_handle: tauri::AppHandle) -> Result<Vec<Project>, String> {
    let state = app_handle.state::<Arc<AppState>>();
    Ok(state.get_projects().await)
}

#[tauri::command]
pub async fn get_project(app_handle: tauri::AppHandle, id: String) -> Result<Project, String> {
    let state = app_handle.state::<Arc<AppState>>();
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    state.get_project(uuid).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_project(
    app_handle: tauri::AppHandle,
    project: Project,
) -> Result<Project, String> {
    let state = app_handle.state::<Arc<AppState>>();
    state
        .update_project(project)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_project(app_handle: tauri::AppHandle, id: String) -> Result<(), String> {
    let state = app_handle.state::<Arc<AppState>>();
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    // Get project name for logging
    let project = state.get_project(uuid).await.ok();

    state.delete_project(uuid).await.map_err(|e| e.to_string())?;

    if let Some(p) = project {
        let log = LogEntry::info(None, LogSource::System, format!("Deleted project: {}", p.name));
        state.add_log(log.clone()).await;
        app_handle.emit("log", &log).ok();
    }

    Ok(())
}

#[tauri::command]
pub async fn link_supabase_project(
    app_handle: tauri::AppHandle,
    project_id: String,
    supabase_project_ref: String,
) -> Result<Project, String> {
    let state = app_handle.state::<Arc<AppState>>();
    let uuid = Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;

    // Verify the remote project exists
    let api = state.get_api_client().await.map_err(|e| e.to_string())?;
    let remote = api
        .get_project(&supabase_project_ref)
        .await
        .map_err(|e| format!("Failed to verify Supabase project: {}", e))?;

    let mut project = state.get_project(uuid).await.map_err(|e| e.to_string())?;

    project.supabase_project_ref = Some(supabase_project_ref.clone());
    project.supabase_project_id = Some(remote.id);
    project.updated_at = chrono::Utc::now();

    let result = state
        .update_project(project)
        .await
        .map_err(|e| e.to_string())?;

    let log = LogEntry::success(
        Some(uuid),
        LogSource::System,
        format!("Linked to Supabase project: {}", remote.name),
    );
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    Ok(result)
}

// Watcher commands

#[tauri::command]
pub async fn start_watching(
    app_handle: tauri::AppHandle,
    project_id: String,
) -> Result<(), String> {
    let state = app_handle.state::<Arc<AppState>>();
    let uuid = Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;

    let project = state.get_project(uuid).await.map_err(|e| e.to_string())?;

    watcher::start_watching(&app_handle, uuid, &project.local_path)
}

#[tauri::command]
pub async fn stop_watching(app_handle: tauri::AppHandle, project_id: String) -> Result<(), String> {
    let uuid = Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;
    watcher::stop_watching(&app_handle, uuid)
}

#[tauri::command]
pub async fn is_watching(app_handle: tauri::AppHandle, project_id: String) -> Result<bool, String> {
    let state = app_handle.state::<Arc<AppState>>();
    let uuid = Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;
    Ok(state.is_watching(uuid).await)
}

// Log commands

#[tauri::command]
pub async fn get_logs(
    app_handle: tauri::AppHandle,
    project_id: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<LogEntry>, String> {
    let state = app_handle.state::<Arc<AppState>>();

    let uuid = match project_id {
        Some(id) => Some(Uuid::parse_str(&id).map_err(|e| e.to_string())?),
        None => None,
    };

    Ok(state.get_logs(uuid, limit.unwrap_or(100)).await)
}

#[tauri::command]
pub async fn clear_logs(
    app_handle: tauri::AppHandle,
    project_id: Option<String>,
) -> Result<(), String> {
    let state = app_handle.state::<Arc<AppState>>();

    let uuid = match project_id {
        Some(id) => Some(Uuid::parse_str(&id).map_err(|e| e.to_string())?),
        None => None,
    };

    state.clear_logs(uuid).await;
    Ok(())
}

// Supabase API commands

#[tauri::command]
pub async fn run_query(
    app_handle: tauri::AppHandle,
    project_id: String,
    query: String,
    read_only: Option<bool>,
) -> Result<serde_json::Value, String> {
    let state = app_handle.state::<Arc<AppState>>();
    let uuid = Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;

    let project = state.get_project(uuid).await.map_err(|e| e.to_string())?;
    let project_ref = project
        .supabase_project_ref
        .ok_or("Project not linked to Supabase")?;

    let api = state.get_api_client().await.map_err(|e| e.to_string())?;

    let log = LogEntry::info(
        Some(uuid),
        LogSource::Schema,
        "Running SQL query...".to_string(),
    );
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    let result = api
        .run_query(&project_ref, &query, read_only.unwrap_or(false))
        .await
        .map_err(|e| {
            let log = LogEntry::error(
                Some(uuid),
                LogSource::Schema,
                format!("Query failed: {}", e),
            );
            tauri::async_runtime::block_on(async {
                state.add_log(log.clone()).await;
            });
            app_handle.emit("log", &log).ok();
            e.to_string()
        })?;

    if let Some(error) = result.error {
        let log = LogEntry::error(Some(uuid), LogSource::Schema, format!("Query error: {}", error));
        state.add_log(log.clone()).await;
        app_handle.emit("log", &log).ok();
        return Err(error);
    }

    let log = LogEntry::success(
        Some(uuid),
        LogSource::Schema,
        "Query executed successfully".to_string(),
    );
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    Ok(result.result.unwrap_or(serde_json::Value::Null))
}

#[tauri::command]
pub async fn deploy_edge_function(
    app_handle: tauri::AppHandle,
    project_id: String,
    function_slug: String,
    function_name: String,
    function_path: String,
) -> Result<String, String> {
    let state = app_handle.state::<Arc<AppState>>();
    let uuid = Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;

    let project = state.get_project(uuid).await.map_err(|e| e.to_string())?;
    let project_ref = project
        .supabase_project_ref
        .ok_or("Project not linked to Supabase")?;

    let api = state.get_api_client().await.map_err(|e| e.to_string())?;

    let log = LogEntry::info(
        Some(uuid),
        LogSource::EdgeFunction,
        format!("Deploying edge function: {}", function_name),
    );
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    // Read the function file
    let full_path = Path::new(&project.local_path).join(&function_path);
    let function_code = tokio::fs::read(&full_path)
        .await
        .map_err(|e| format!("Failed to read function file: {}", e))?;

    // Get entrypoint from path
    let entrypoint = Path::new(&function_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("index.ts");

    let result = api
        .deploy_function(
            &project_ref,
            &function_slug,
            &function_name,
            entrypoint,
            function_code,
        )
        .await
        .map_err(|e| {
            let log = LogEntry::error(
                Some(uuid),
                LogSource::EdgeFunction,
                format!("Deploy failed: {}", e),
            );
            tauri::async_runtime::block_on(async {
                state.add_log(log.clone()).await;
            });
            app_handle.emit("log", &log).ok();
            e.to_string()
        })?;

    let log = LogEntry::success(
        Some(uuid),
        LogSource::EdgeFunction,
        format!("Deployed {} (v{})", result.name, result.version),
    );
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    Ok(format!(
        "Successfully deployed {} version {}",
        result.name, result.version
    ))
}

#[tauri::command]
pub async fn get_remote_schema(
    app_handle: tauri::AppHandle,
    project_id: String,
) -> Result<String, String> {
    let state = app_handle.state::<Arc<AppState>>();
    let uuid = Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;

    let project = state.get_project(uuid).await.map_err(|e| e.to_string())?;
    let project_ref = project
        .supabase_project_ref
        .ok_or("Project not linked to Supabase")?;

    let api = state.get_api_client().await.map_err(|e| e.to_string())?;

    let log = LogEntry::info(
        Some(uuid),
        LogSource::Schema,
        "Fetching remote schema...".to_string(),
    );
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    let schema = api
        .get_schema(&project_ref)
        .await
        .map_err(|e| e.to_string())?;

    let log = LogEntry::success(
        Some(uuid),
        LogSource::Schema,
        "Remote schema fetched".to_string(),
    );
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    Ok(schema)
}
