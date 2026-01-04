use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::models::{AppData, LogEntry, Project};
use crate::supabase_api::SupabaseApi;

#[derive(Error, Debug)]
pub enum StateError {
    #[error("Project not found: {0}")]
    ProjectNotFound(Uuid),
    #[error("Failed to read data file: {0}")]
    ReadError(String),
    #[error("Failed to write data file: {0}")]
    WriteError(String),
    #[error("Failed to parse data: {0}")]
    ParseError(String),
    #[error("Access token not configured")]
    NoAccessToken,
}

pub type WatcherHandle = notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>;

pub struct AppState {
    pub data: RwLock<AppData>,
    pub logs: RwLock<Vec<LogEntry>>,
    pub watchers: RwLock<HashMap<Uuid, WatcherHandle>>,
    data_path: PathBuf,
}

impl AppState {
    pub fn new() -> Self {
        let data_path = Self::get_data_path();
        let data = Self::load_data(&data_path).unwrap_or_default();

        Self {
            data: RwLock::new(data),
            logs: RwLock::new(Vec::new()),
            watchers: RwLock::new(HashMap::new()),
            data_path,
        }
    }

    fn get_data_path() -> PathBuf {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("supawatch");

        fs::create_dir_all(&data_dir).ok();
        data_dir.join("data.json")
    }

    fn load_data(path: &PathBuf) -> Result<AppData, StateError> {
        if !path.exists() {
            return Ok(AppData::default());
        }

        let content = fs::read_to_string(path)
            .map_err(|e| StateError::ReadError(e.to_string()))?;

        serde_json::from_str(&content)
            .map_err(|e| StateError::ParseError(e.to_string()))
    }

    pub async fn save(&self) -> Result<(), StateError> {
        let data = self.data.read().await;
        let content = serde_json::to_string_pretty(&*data)
            .map_err(|e| StateError::ParseError(e.to_string()))?;

        fs::write(&self.data_path, content)
            .map_err(|e| StateError::WriteError(e.to_string()))
    }

    // Project operations
    pub async fn add_project(&self, project: Project) -> Result<Project, StateError> {
        let mut data = self.data.write().await;
        let project_clone = project.clone();
        data.projects.push(project);
        drop(data);
        self.save().await?;
        Ok(project_clone)
    }

    pub async fn get_projects(&self) -> Vec<Project> {
        let data = self.data.read().await;
        data.projects.clone()
    }

    pub async fn get_project(&self, id: Uuid) -> Result<Project, StateError> {
        let data = self.data.read().await;
        data.projects
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or(StateError::ProjectNotFound(id))
    }

    pub async fn update_project(&self, project: Project) -> Result<Project, StateError> {
        let mut data = self.data.write().await;
        let idx = data
            .projects
            .iter()
            .position(|p| p.id == project.id)
            .ok_or(StateError::ProjectNotFound(project.id))?;

        data.projects[idx] = project.clone();
        drop(data);
        self.save().await?;
        Ok(project)
    }

    pub async fn delete_project(&self, id: Uuid) -> Result<(), StateError> {
        // Stop watcher if running
        self.stop_watcher(id).await;

        let mut data = self.data.write().await;
        let idx = data
            .projects
            .iter()
            .position(|p| p.id == id)
            .ok_or(StateError::ProjectNotFound(id))?;

        data.projects.remove(idx);
        drop(data);
        self.save().await
    }

    pub async fn set_project_watching(&self, id: Uuid, watching: bool) -> Result<(), StateError> {
        let mut data = self.data.write().await;
        let project = data
            .projects
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or(StateError::ProjectNotFound(id))?;

        project.is_watching = watching;
        project.updated_at = chrono::Utc::now();
        drop(data);
        self.save().await
    }

    // Watcher operations
    pub async fn add_watcher(&self, project_id: Uuid, watcher: WatcherHandle) {
        let mut watchers = self.watchers.write().await;
        watchers.insert(project_id, watcher);
    }

    pub async fn stop_watcher(&self, project_id: Uuid) {
        let mut watchers = self.watchers.write().await;
        watchers.remove(&project_id);
    }

    pub async fn is_watching(&self, project_id: Uuid) -> bool {
        let watchers = self.watchers.read().await;
        watchers.contains_key(&project_id)
    }

    // Log operations
    pub async fn add_log(&self, log: LogEntry) {
        let mut logs = self.logs.write().await;
        logs.push(log);

        // Keep only last 1000 logs
        if logs.len() > 1000 {
            let drain_count = logs.len() - 1000;
            logs.drain(0..drain_count);
        }
    }

    pub async fn get_logs(&self, project_id: Option<Uuid>, limit: usize) -> Vec<LogEntry> {
        let logs = self.logs.read().await;
        let filtered: Vec<_> = logs
            .iter()
            .filter(|log| {
                project_id.is_none() || log.project_id == project_id
            })
            .cloned()
            .collect();

        filtered.into_iter().rev().take(limit).collect()
    }

    pub async fn clear_logs(&self, project_id: Option<Uuid>) {
        let mut logs = self.logs.write().await;
        if let Some(pid) = project_id {
            logs.retain(|log| log.project_id != Some(pid));
        } else {
            logs.clear();
        }
    }

    // Access token operations
    pub async fn set_access_token(&self, token: String) -> Result<(), StateError> {
        let mut data = self.data.write().await;
        data.access_token = Some(token);
        drop(data);
        self.save().await
    }

    pub async fn get_access_token(&self) -> Option<String> {
        let data = self.data.read().await;
        data.access_token.clone()
    }

    pub async fn clear_access_token(&self) -> Result<(), StateError> {
        let mut data = self.data.write().await;
        data.access_token = None;
        drop(data);
        self.save().await
    }

    pub async fn has_access_token(&self) -> bool {
        let data = self.data.read().await;
        data.access_token.is_some()
    }

    /// Get a Supabase API client using the stored access token
    pub async fn get_api_client(&self) -> Result<SupabaseApi, StateError> {
        let token = self.get_access_token().await.ok_or(StateError::NoAccessToken)?;
        Ok(SupabaseApi::new(token))
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
