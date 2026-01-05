import { invoke } from "@tauri-apps/api/core";
import type { LogEntry, Project, RemoteProject } from "./types";

// Access Token API
export async function setAccessToken(token: string): Promise<void> {
  return invoke("set_access_token", { token });
}

export async function hasAccessToken(): Promise<boolean> {
  return invoke("has_access_token");
}

export async function clearAccessToken(): Promise<void> {
  return invoke("clear_access_token");
}

export async function validateAccessToken(): Promise<boolean> {
  return invoke("validate_access_token");
}

// Remote Supabase Projects API
export async function listRemoteProjects(): Promise<RemoteProject[]> {
  return invoke("list_remote_projects");
}

export async function listOrganizations(): Promise<
  import("./types").Organization[]
> {
  return invoke("list_organizations");
}

// Project API
export async function createProject(
  name: string,
  localPath: string,
  supabaseProjectId?: string,
  supabaseProjectRef?: string,
  organizationId?: string
): Promise<Project> {
  return invoke("create_project", {
    name,
    localPath,
    supabaseProjectId,
    supabaseProjectRef,
    organizationId,
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

export async function linkSupabaseProject(
  projectId: string,
  supabaseProjectRef: string
): Promise<Project> {
  return invoke("link_supabase_project", { projectId, supabaseProjectRef });
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

// Supabase API
export async function runQuery(
  projectId: string,
  query: string,
  readOnly?: boolean
): Promise<unknown> {
  return invoke("run_query", { projectId, query, readOnly });
}

export async function deployEdgeFunction(
  projectId: string,
  functionSlug: string,
  functionName: string,
  functionPath: string
): Promise<string> {
  return invoke("deploy_edge_function", {
    projectId,
    functionSlug,
    functionName,
    functionPath,
  });
}

export async function getRemoteSchema(projectId: string): Promise<string> {
  return invoke("get_remote_schema", { projectId });
}

export async function pullProject(projectId: string): Promise<void> {
  return invoke("pull_project", { projectId });
}

// Supabase Logs API
export async function querySupabaseLogs(
  projectId: string,
  sql?: string,
  isoTimestampStart?: string,
  isoTimestampEnd?: string
): Promise<unknown> {
  return invoke("query_supabase_logs", {
    projectId,
    sql,
    isoTimestampStart,
    isoTimestampEnd,
  });
}

export async function getEdgeFunctionLogs(
  projectId: string,
  functionName?: string,
  minutes?: number
): Promise<unknown> {
  return invoke("get_edge_function_logs", {
    projectId,
    functionName,
    minutes,
  });
}

export async function getPostgresLogs(
  projectId: string,
  minutes?: number
): Promise<unknown> {
  return invoke("get_postgres_logs", { projectId, minutes });
}

export async function getAuthLogs(
  projectId: string,
  minutes?: number
): Promise<unknown> {
  return invoke("get_auth_logs", { projectId, minutes });
}
