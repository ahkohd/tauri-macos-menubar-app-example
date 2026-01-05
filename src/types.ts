export interface Project {
  id: string;
  name: string;
  local_path: string;
  supabase_project_id: string | null;
  supabase_project_ref: string | null;
  created_at: string;
  updated_at: string;
  is_watching: boolean;
}

export type LogLevel = "info" | "warning" | "error" | "success";

export type LogSource = "schema" | "edge_function" | "watcher" | "system";

export interface LogEntry {
  id: string;
  project_id: string | null;
  level: LogLevel;
  source: LogSource;
  message: string;
  details: string | null;
  timestamp: string;
}

export type FileChangeType = "schema" | "edge_function" | "migration" | "other";

export interface FileChange {
  path: string;
  change_type: FileChangeType;
  project_id: string;
  timestamp: string;
}

export type Tab = "projects" | "logs" | "settings";

export interface RemoteProject {
  id: string;
  name: string;
  organization_id: string;
  region: string;
  created_at: string;
}

export interface Organization {
  id: string;
  name: string;
}
