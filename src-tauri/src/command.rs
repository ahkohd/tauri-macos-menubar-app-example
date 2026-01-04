use std::sync::{Arc, Once};

use tauri::Manager;
use tauri_nspanel::ManagerExt;
use uuid::Uuid;

use crate::fns::{
    setup_menubar_panel_listeners, swizzle_to_menubar_panel, update_menubar_appearance,
};
use crate::models::{LogEntry, LogSource, Project};
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
    state.add_log(log).await;

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
        state.add_log(log).await;
    }

    Ok(())
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

// Supabase CLI commands

#[tauri::command]
pub async fn deploy_edge_function(
    app_handle: tauri::AppHandle,
    project_id: String,
    function_name: String,
) -> Result<String, String> {
    let state = app_handle.state::<Arc<AppState>>();
    let uuid = Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;

    let project = state.get_project(uuid).await.map_err(|e| e.to_string())?;

    let project_ref = project
        .supabase_project_ref
        .ok_or("Project not linked to Supabase")?;

    let log = LogEntry::info(
        Some(uuid),
        LogSource::EdgeFunction,
        format!("Deploying edge function: {}", function_name),
    );
    state.add_log(log).await;

    // Execute supabase functions deploy command
    let output = std::process::Command::new("supabase")
        .args([
            "functions",
            "deploy",
            &function_name,
            "--project-ref",
            &project_ref,
        ])
        .current_dir(&project.local_path)
        .output()
        .map_err(|e| format!("Failed to execute supabase CLI: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        let log = LogEntry::success(
            Some(uuid),
            LogSource::EdgeFunction,
            format!("Successfully deployed: {}", function_name),
        )
        .with_details(stdout.clone());
        state.add_log(log).await;
        Ok(stdout)
    } else {
        let log = LogEntry::error(
            Some(uuid),
            LogSource::EdgeFunction,
            format!("Failed to deploy: {}", function_name),
        )
        .with_details(stderr.clone());
        state.add_log(log).await;
        Err(stderr)
    }
}

#[tauri::command]
pub async fn run_migration(
    app_handle: tauri::AppHandle,
    project_id: String,
    sql: String,
) -> Result<String, String> {
    let state = app_handle.state::<Arc<AppState>>();
    let uuid = Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;

    let project = state.get_project(uuid).await.map_err(|e| e.to_string())?;

    let project_ref = project
        .supabase_project_ref
        .ok_or("Project not linked to Supabase")?;

    let log = LogEntry::info(
        Some(uuid),
        LogSource::Schema,
        "Running migration...".to_string(),
    );
    state.add_log(log).await;

    // Execute supabase db push or run SQL directly
    let output = std::process::Command::new("supabase")
        .args(["db", "push", "--project-ref", &project_ref])
        .current_dir(&project.local_path)
        .output()
        .map_err(|e| format!("Failed to execute supabase CLI: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        let log = LogEntry::success(
            Some(uuid),
            LogSource::Schema,
            "Migration completed successfully".to_string(),
        )
        .with_details(stdout.clone());
        state.add_log(log).await;
        Ok(stdout)
    } else {
        let log = LogEntry::error(
            Some(uuid),
            LogSource::Schema,
            "Migration failed".to_string(),
        )
        .with_details(stderr.clone());
        state.add_log(log).await;
        Err(stderr)
    }
}

#[tauri::command]
pub async fn link_supabase_project(
    app_handle: tauri::AppHandle,
    project_id: String,
    supabase_project_ref: String,
) -> Result<Project, String> {
    let state = app_handle.state::<Arc<AppState>>();
    let uuid = Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;

    let mut project = state.get_project(uuid).await.map_err(|e| e.to_string())?;

    // Run supabase link command
    let output = std::process::Command::new("supabase")
        .args(["link", "--project-ref", &supabase_project_ref])
        .current_dir(&project.local_path)
        .output()
        .map_err(|e| format!("Failed to execute supabase CLI: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("Failed to link project: {}", stderr));
    }

    project.supabase_project_ref = Some(supabase_project_ref.clone());
    project.updated_at = chrono::Utc::now();

    let result = state
        .update_project(project)
        .await
        .map_err(|e| e.to_string())?;

    let log = LogEntry::success(
        Some(uuid),
        LogSource::System,
        format!("Linked to Supabase project: {}", supabase_project_ref),
    );
    state.add_log(log).await;

    Ok(result)
}

#[tauri::command]
pub async fn init_supabase_project(
    app_handle: tauri::AppHandle,
    project_id: String,
) -> Result<String, String> {
    let state = app_handle.state::<Arc<AppState>>();
    let uuid = Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;

    let project = state.get_project(uuid).await.map_err(|e| e.to_string())?;

    let log = LogEntry::info(
        Some(uuid),
        LogSource::System,
        "Initializing Supabase project...".to_string(),
    );
    state.add_log(log).await;

    // Run supabase init command
    let output = std::process::Command::new("supabase")
        .args(["init"])
        .current_dir(&project.local_path)
        .output()
        .map_err(|e| format!("Failed to execute supabase CLI: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        let log = LogEntry::success(
            Some(uuid),
            LogSource::System,
            "Supabase project initialized".to_string(),
        );
        state.add_log(log).await;
        Ok(stdout)
    } else {
        let log = LogEntry::error(
            Some(uuid),
            LogSource::System,
            "Failed to initialize Supabase project".to_string(),
        )
        .with_details(stderr.clone());
        state.add_log(log).await;
        Err(stderr)
    }
}
