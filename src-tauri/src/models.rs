use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub local_path: String,
    pub supabase_project_id: Option<String>,
    pub supabase_project_ref: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub is_watching: bool,
}

impl Project {
    pub fn new(name: String, local_path: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            local_path,
            supabase_project_id: None,
            supabase_project_ref: None,
            created_at: now,
            updated_at: now,
            is_watching: false,
        }
    }

    pub fn with_remote(
        name: String,
        local_path: String,
        project_id: String,
        project_ref: String,
    ) -> Self {
        let mut project = Self::new(name, local_path);
        project.supabase_project_id = Some(project_id);
        project.supabase_project_ref = Some(project_ref);
        project
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Info,
    Warning,
    Error,
    Success,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogSource {
    Schema,
    EdgeFunction,
    Watcher,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub level: LogLevel,
    pub source: LogSource,
    pub message: String,
    pub details: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl LogEntry {
    pub fn new(
        project_id: Option<Uuid>,
        level: LogLevel,
        source: LogSource,
        message: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id,
            level,
            source,
            message,
            details: None,
            timestamp: Utc::now(),
        }
    }

    pub fn with_details(mut self, details: String) -> Self {
        self.details = Some(details);
        self
    }

    pub fn info(project_id: Option<Uuid>, source: LogSource, message: String) -> Self {
        Self::new(project_id, LogLevel::Info, source, message)
    }

    pub fn warning(project_id: Option<Uuid>, source: LogSource, message: String) -> Self {
        Self::new(project_id, LogLevel::Warning, source, message)
    }

    pub fn error(project_id: Option<Uuid>, source: LogSource, message: String) -> Self {
        Self::new(project_id, LogLevel::Error, source, message)
    }

    pub fn success(project_id: Option<Uuid>, source: LogSource, message: String) -> Self {
        Self::new(project_id, LogLevel::Success, source, message)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeType {
    Schema,
    EdgeFunction,
    Migration,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub change_type: FileChangeType,
    pub project_id: Uuid,
    pub timestamp: DateTime<Utc>,
}

impl FileChange {
    pub fn new(path: String, change_type: FileChangeType, project_id: Uuid) -> Self {
        Self {
            path,
            change_type,
            project_id,
            timestamp: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppData {
    pub projects: Vec<Project>,
    /// Supabase personal access token (stored encrypted in production)
    #[serde(default)]
    pub access_token: Option<String>,
    pub version: String,
}

impl Default for AppData {
    fn default() -> Self {
        Self {
            projects: Vec::new(),
            access_token: None,
            version: "1.0.0".to_string(),
        }
    }
}

/// Remote Supabase project info from the API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteProject {
    pub id: String,
    pub name: String,
    pub organization_id: String,
    pub region: String,
    pub created_at: String,
}
