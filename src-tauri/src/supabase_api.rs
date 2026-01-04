use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const SUPABASE_API_BASE: &str = "https://api.supabase.com";

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },
    #[error("Missing access token")]
    MissingToken,
    #[error("Missing project reference")]
    MissingProjectRef,
    #[error("File read error: {0}")]
    FileReadError(String),
}

#[derive(Debug, Serialize)]
struct QueryRequest {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    read_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct QueryResponse {
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub organization_id: String,
    pub region: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct EdgeFunction {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub status: String,
    pub version: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
struct FunctionMetadata {
    entrypoint_path: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    verify_jwt: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct DeployResponse {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub version: i32,
}

pub struct SupabaseApi {
    client: reqwest::Client,
    access_token: String,
}

impl SupabaseApi {
    pub fn new(access_token: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            access_token,
        }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.access_token)
    }

    /// List all projects accessible by the access token
    pub async fn list_projects(&self) -> Result<Vec<Project>, ApiError> {
        let url = format!("{}/v1/projects", SUPABASE_API_BASE);

        let response = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            return Err(ApiError::ApiError { status, message });
        }

        Ok(response.json().await?)
    }

    /// Get a specific project by reference
    pub async fn get_project(&self, project_ref: &str) -> Result<Project, ApiError> {
        let url = format!("{}/v1/projects/{}", SUPABASE_API_BASE, project_ref);

        let response = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            return Err(ApiError::ApiError { status, message });
        }

        Ok(response.json().await?)
    }

    /// Run a SQL query against the project's database
    pub async fn run_query(
        &self,
        project_ref: &str,
        query: &str,
        read_only: bool,
    ) -> Result<QueryResponse, ApiError> {
        let url = format!(
            "{}/v1/projects/{}/database/query",
            SUPABASE_API_BASE, project_ref
        );

        let body = QueryRequest {
            query: query.to_string(),
            read_only: Some(read_only),
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            return Err(ApiError::ApiError { status, message });
        }

        Ok(response.json().await?)
    }

    /// List all edge functions for a project
    pub async fn list_functions(&self, project_ref: &str) -> Result<Vec<EdgeFunction>, ApiError> {
        let url = format!("{}/v1/projects/{}/functions", SUPABASE_API_BASE, project_ref);

        let response = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            return Err(ApiError::ApiError { status, message });
        }

        Ok(response.json().await?)
    }

    /// Deploy an edge function
    ///
    /// The function_code should be the TypeScript/JavaScript source code.
    /// entrypoint is the main file name (e.g., "index.ts")
    pub async fn deploy_function(
        &self,
        project_ref: &str,
        slug: &str,
        name: &str,
        entrypoint: &str,
        function_code: Vec<u8>,
    ) -> Result<DeployResponse, ApiError> {
        let url = format!(
            "{}/v1/projects/{}/functions/deploy?slug={}",
            SUPABASE_API_BASE, project_ref, slug
        );

        let metadata = FunctionMetadata {
            entrypoint_path: entrypoint.to_string(),
            name: name.to_string(),
            verify_jwt: Some(true),
        };

        let metadata_json = serde_json::to_string(&metadata)
            .map_err(|e| ApiError::FileReadError(e.to_string()))?;

        let form = Form::new()
            .text("metadata", metadata_json)
            .part(
                "file",
                Part::bytes(function_code)
                    .file_name(entrypoint.to_string())
                    .mime_str("application/typescript")
                    .map_err(|e| ApiError::FileReadError(e.to_string()))?,
            );

        let response = self
            .client
            .post(&url)
            .header("Authorization", self.auth_header())
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            return Err(ApiError::ApiError { status, message });
        }

        Ok(response.json().await?)
    }

    /// Delete an edge function
    pub async fn delete_function(
        &self,
        project_ref: &str,
        function_slug: &str,
    ) -> Result<(), ApiError> {
        let url = format!(
            "{}/v1/projects/{}/functions/{}",
            SUPABASE_API_BASE, project_ref, function_slug
        );

        let response = self
            .client
            .delete(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            return Err(ApiError::ApiError { status, message });
        }

        Ok(())
    }

    /// Get the current database schema (useful for diffing)
    pub async fn get_schema(&self, project_ref: &str) -> Result<String, ApiError> {
        // Query to get the current schema
        let query = r#"
            SELECT
                table_schema,
                table_name,
                column_name,
                data_type,
                is_nullable,
                column_default
            FROM information_schema.columns
            WHERE table_schema NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
            ORDER BY table_schema, table_name, ordinal_position;
        "#;

        let result = self.run_query(project_ref, query, true).await?;

        Ok(serde_json::to_string_pretty(&result.result).unwrap_or_default())
    }
}
