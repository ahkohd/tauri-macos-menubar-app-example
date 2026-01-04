import { invoke } from "@tauri-apps/api/core";
import type { Project, LogEntry } from "./types";

// Project API
export async function createProject(
  name: string,
  localPath: string,
  supabaseProjectId?: string,
  supabaseProjectRef?: string
): Promise<Project> {
  return invoke("create_project", {
    name,
    localPath,
    supabaseProjectId,
    supabaseProjectRef,
  });
}

export async function getProjects(): Promise<Project[]> {
  return invoke("get_projects");
}

export async function getProject(id: string): Promise<Project> {
  return invoke("get_project", { id });
}

export async function updateProject(project: Project): Promise<Project> {
  return invoke("update_project", { project });
}

export async function deleteProject(id: string): Promise<void> {
  return invoke("delete_project", { id });
}

// Watcher API
export async function startWatching(projectId: string): Promise<void> {
  return invoke("start_watching", { projectId });
}

export async function stopWatching(projectId: string): Promise<void> {
  return invoke("stop_watching", { projectId });
}

export async function isWatching(projectId: string): Promise<boolean> {
  return invoke("is_watching", { projectId });
}

// Logs API
export async function getLogs(
  projectId?: string,
  limit?: number
): Promise<LogEntry[]> {
  return invoke("get_logs", { projectId, limit });
}

export async function clearLogs(projectId?: string): Promise<void> {
  return invoke("clear_logs", { projectId });
}

// Supabase CLI API
export async function deployEdgeFunction(
  projectId: string,
  functionName: string
): Promise<string> {
  return invoke("deploy_edge_function", { projectId, functionName });
}

export async function runMigration(
  projectId: string,
  sql: string
): Promise<string> {
  return invoke("run_migration", { projectId, sql });
}

export async function linkSupabaseProject(
  projectId: string,
  supabaseProjectRef: string
): Promise<Project> {
  return invoke("link_supabase_project", { projectId, supabaseProjectRef });
}

export async function initSupabaseProject(projectId: string): Promise<string> {
  return invoke("init_supabase_project", { projectId });
}
