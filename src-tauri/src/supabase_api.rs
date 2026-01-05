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

#[derive(Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub organization_id: String,
    pub region: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Organization {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
struct CreateProjectBody {
    name: String,
    organization_id: String,
    db_pass: String,
    region: String,
    plan: String,
}

#[derive(Debug, Deserialize)]
pub struct EdgeFunction {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub status: String,
    pub version: i32,
    pub created_at: serde_json::Value,
    pub updated_at: serde_json::Value,
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

pub struct FunctionBody {
    pub content_type: String,
    pub data: Vec<u8>,
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

    /// List all organizations
    pub async fn list_organizations(&self) -> Result<Vec<Organization>, ApiError> {
        let url = format!("{}/v1/organizations", SUPABASE_API_BASE);

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

    /// Create a new project
    pub async fn create_project(
        &self,
        name: &str,
        organization_id: &str,
        db_pass: &str,
        region: &str,
    ) -> Result<Project, ApiError> {
        let url = format!("{}/v1/projects", SUPABASE_API_BASE);

        let body = CreateProjectBody {
            name: name.to_string(),
            organization_id: organization_id.to_string(),
            db_pass: db_pass.to_string(),
            region: region.to_string(),
            plan: "free".to_string(), // Defaulting to free plan
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

        let body_text = response.text().await?;
        
        let val: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|e| {
                 let snippet: String = body_text.chars().take(200).collect();
                 ApiError::ApiError { 
                     status: 200, 
                     message: format!("Failed to parse JSON: {}. Body: {}", e, snippet) 
                 }
            })?;

        if val.is_array() {
            return Ok(QueryResponse {
                result: Some(val),
                error: None,
            });
        }

        if let serde_json::Value::Object(ref map) = val {
            if map.contains_key("result") || map.contains_key("error") {
                 // Try standard deserialization
                 if let Ok(resp) = serde_json::from_value::<QueryResponse>(val.clone()) {
                      // Validate inner result is array if present
                      if let Some(res) = &resp.result {
                          if !res.is_array() {
                               return Err(ApiError::ApiError { 
                                     status: 200, 
                                     message: format!("Query 'result' is not an array: {:?}. Body: {}", res, body_text)
                               });
                          }
                      }
                      return Ok(resp);
                 }
            }
        }
        
        // If we reached here, it's an object but not a query response, or failed to deserialize
        let snippet: String = body_text.chars().take(200).collect();
        Err(ApiError::ApiError { 
             status: 200, 
             message: format!("Unexpected response format. Not an array and not a result/error object. Body: {}", snippet) 
        })
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

        let body_text = response.text().await?;
        match serde_json::from_str::<Vec<EdgeFunction>>(&body_text) {
            Ok(funcs) => Ok(funcs),
            Err(e) => {
                let snippet: String = body_text.chars().take(200).collect();
                Err(ApiError::ApiError { 
                    status: 200, 
                    message: format!("Failed to parse functions list: {}. Body: {}", e, snippet) 
                })
            }
        }
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

    /// Get edge function body (code)
    pub async fn get_function_body(
        &self,
        project_ref: &str,
        function_slug: &str,
    ) -> Result<FunctionBody, ApiError> {
        let url = format!(
            "{}/v1/projects/{}/functions/{}/body",
            SUPABASE_API_BASE, project_ref, function_slug
        );

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

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        let data = response.bytes().await?.to_vec();

        Ok(FunctionBody { content_type, data })
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

    /// Query project logs using SQL
    ///
    /// Available log sources: edge_logs, postgres_logs, auth_logs, realtime_logs, storage_logs, postgrest_logs
    /// If no SQL provided, defaults to querying edge_logs
    /// Timestamp range must be no more than 24 hours
    pub async fn query_logs(
        &self,
        project_ref: &str,
        sql: Option<&str>,
        iso_timestamp_start: Option<&str>,
        iso_timestamp_end: Option<&str>,
    ) -> Result<serde_json::Value, ApiError> {
        let mut url = format!(
            "{}/v1/projects/{}/analytics/endpoints/logs.all/query",
            SUPABASE_API_BASE, project_ref
        );

        let mut query_params = Vec::new();
        if let Some(sql) = sql {
            query_params.push(format!("sql={}", urlencoding::encode(sql)));
        }
        if let Some(start) = iso_timestamp_start {
            query_params.push(format!("iso_timestamp_start={}", urlencoding::encode(start)));
        }
        if let Some(end) = iso_timestamp_end {
            query_params.push(format!("iso_timestamp_end={}", urlencoding::encode(end)));
        }

        if !query_params.is_empty() {
            url = format!("{}?{}", url, query_params.join("&"));
        }

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

    /// Get edge function logs for the last N minutes
    pub async fn get_edge_function_logs(
        &self,
        project_ref: &str,
        function_name: Option<&str>,
        minutes: u32,
    ) -> Result<serde_json::Value, ApiError> {
        let now = chrono::Utc::now();
        let start = now - chrono::Duration::minutes(minutes as i64);

        let sql = if let Some(name) = function_name {
            format!(
                r#"select id, timestamp, event_message, metadata, request
                   from edge_logs
                   where metadata->>'function_id' = '{}'
                   order by timestamp desc
                   limit 100"#,
                name
            )
        } else {
            r#"select id, timestamp, event_message, metadata, request
               from edge_logs
               order by timestamp desc
               limit 100"#
                .to_string()
        };

        self.query_logs(
            project_ref,
            Some(&sql),
            Some(&start.to_rfc3339()),
            Some(&now.to_rfc3339()),
        )
        .await
    }

    /// Get postgres logs for the last N minutes
    pub async fn get_postgres_logs(
        &self,
        project_ref: &str,
        minutes: u32,
    ) -> Result<serde_json::Value, ApiError> {
        let now = chrono::Utc::now();
        let start = now - chrono::Duration::minutes(minutes as i64);

        let sql = r#"select id, timestamp, event_message, error_severity, user_name, query
                     from postgres_logs
                     order by timestamp desc
                     limit 100"#;

        self.query_logs(
            project_ref,
            Some(sql),
            Some(&start.to_rfc3339()),
            Some(&now.to_rfc3339()),
        )
        .await
    }

    /// Get auth logs for the last N minutes
    pub async fn get_auth_logs(
        &self,
        project_ref: &str,
        minutes: u32,
    ) -> Result<serde_json::Value, ApiError> {
        let now = chrono::Utc::now();
        let start = now - chrono::Duration::minutes(minutes as i64);

        let sql = r#"select id, timestamp, event_message, level, path, status
                     from auth_logs
                     order by timestamp desc
                     limit 100"#;

        self.query_logs(
            project_ref,
            Some(sql),
            Some(&start.to_rfc3339()),
            Some(&now.to_rfc3339()),
        )
        .await
    }
}
