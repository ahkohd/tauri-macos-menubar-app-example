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
use crate::supabase_api::Organization;
use crate::watcher;
use eszip::EszipV2;

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

// Sync commands

#[tauri::command]
pub async fn list_organizations(app_handle: tauri::AppHandle) -> Result<Vec<Organization>, String> {
    let state = app_handle.state::<Arc<AppState>>();
    let api = state.get_api_client().await.map_err(|e| e.to_string())?;
    api.list_organizations()
        .await
        .map_err(|e| format!("Failed to list organizations: {}", e))
}

#[tauri::command]
pub async fn pull_project(
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

    let log = LogEntry::info(Some(uuid), LogSource::System, "Pulling remote schema...".to_string());
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    // 1. Introspect Remote
    let introspector = crate::introspection::Introspector::new(&api, project_ref.clone());
    let remote_schema = introspector.introspect().await.map_err(|e| e.to_string())?;

    // 2. Generate SQL (Full Dump)
    // We can use the generator to create CREATE statements for everything in remote schema
    // By "diffing" against an empty schema
    let empty_schema = crate::schema::DbSchema::new();
    let diff = crate::diff::compute_diff(&empty_schema, &remote_schema); // Remote is 'local' (target), Empty is 'remote' (base) -> All creates
    // Wait, compute_diff(remote, local) -> diff to transform remote to local?
    // compute_diff(base, target) -> diff to transform base to target.
    // We want to transform Empty -> Remote. So compute_diff(empty, remote).
    
    let sql = crate::generator::generate_sql(&diff, &remote_schema);

    // 3. Write to file
    let supabase_dir = std::path::Path::new(&project.local_path).join("supabase");
    if !supabase_dir.exists() {
        tokio::fs::create_dir_all(&supabase_dir)
            .await
            .map_err(|e| e.to_string())?;
    }
    
    let schemas_dir = supabase_dir.join("schemas");
    let schema_path = schemas_dir.join("schema.sql");
    // Ensure schemas dir exists
    if !schemas_dir.exists() {
         tokio::fs::create_dir_all(&schemas_dir)
            .await
            .map_err(|e| e.to_string())?;
    }

    tokio::fs::write(&schema_path, &sql)
        .await
        .map_err(|e| e.to_string())?;

    let log = LogEntry::success(
        Some(uuid),
        LogSource::System,
        "Project schema pulled to supabase/schemas/schema.sql".to_string(),
    );
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    // 4. Pull Edge Functions
    pull_edge_functions(&api, &project_ref, uuid, std::path::Path::new(&project.local_path), state.inner(), &app_handle).await?;

    Ok(sql)
}

// Helper to pull edge functions
async fn pull_edge_functions(
    api: &crate::supabase_api::SupabaseApi,
    project_ref: &str,
    project_id: Uuid,
    project_local_path: &std::path::Path,
    state: &Arc<AppState>,
    app_handle: &tauri::AppHandle,
) -> Result<(), String> {
    let log = LogEntry::info(
        Some(project_id),
        LogSource::System,
        "Syncing edge functions...".to_string(),
    );
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    match api.list_functions(project_ref).await {
        Ok(funcs) => {
            let supabase_dir = project_local_path.join("supabase");
            let functions_dir = supabase_dir.join("functions");
            if !functions_dir.exists() {
                tokio::fs::create_dir_all(&functions_dir)
                    .await
                    .map_err(|e| e.to_string())?;
            }

            let mut func_count = 0;
            for func in funcs {
                let func_dir = functions_dir.join(&func.slug);
                if !func_dir.exists() {
                    tokio::fs::create_dir_all(&func_dir)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                
                match api.get_function_body(project_ref, &func.slug).await {
                    Ok(body) => {
                        let (filename, is_binary) = if body.content_type == "application/vnd.denoland.eszip" || body.content_type == "application/octet-stream" {
                             ("function.eszip", true)
                        } else {
                             ("index.ts", false)
                        };
                        
                        let mut unpacked = false;
                        if is_binary {
                             let reader = futures::io::Cursor::new(body.data.clone());
                             let buf_reader = futures::io::BufReader::new(reader);
                             
                             if let Ok((eszip, loader)) = eszip::EszipV2::parse(buf_reader).await {
                                 let loader_handle = tokio::spawn(async move {
                                     let _ = loader.await;
                                 });

                                 let specifiers = eszip.specifiers();
                                 for specifier in specifiers {
                                     if specifier.starts_with("file:///") {
                                         if let Some(module) = eszip.get_module(&specifier) {
                                             if let Some(source) = module.source().await {
                                                 let path_str = specifier.trim_start_matches("file:///");
                                                 let output_path = if path_str.ends_with("/index.ts") || path_str == "index.ts" {
                                                      Some("index.ts")
                                                 } else if !path_str.contains("node_modules") && !path_str.contains("deno_dir") {
                                                      std::path::Path::new(path_str).file_name().and_then(|n| n.to_str())
                                                 } else {
                                                      None
                                                 };

                                                 if let Some(out_name) = output_path {
                                                      let _ = tokio::fs::write(func_dir.join(out_name), source).await;
                                                      unpacked = true;
                                                 }
                                             }
                                         }
                                     }
                                 }
                                 loader_handle.abort();
                             }
                             
                             if !unpacked {
                                  let _ = tokio::fs::write(func_dir.join("function.eszip"), &body.data).await;
                                  let notice = format!("// The source code for function '{}' could not be unpacked.\n// The deployed bundle has been downloaded as 'function.eszip'.", func.slug);
                                  let _ = tokio::fs::write(func_dir.join("index.ts"), notice).await;
                             }
                        } else {
                             let file_path = func_dir.join("index.ts");
                             if let Err(e) = tokio::fs::write(&file_path, &body.data).await {
                                  let log = LogEntry::error(Some(project_id), LogSource::System, format!("Failed to save function {}: {}", func.slug, e));
                                  state.add_log(log).await;
                             }
                        }
                        func_count += 1;
                    },
                    Err(e) => {
                         let log = LogEntry::error(Some(project_id), LogSource::System, format!("Failed to download function {}: {}", func.slug, e));
                         state.add_log(log).await;
                    }
                }
            }
            
            if func_count > 0 {
                let log = LogEntry::success(
                    Some(project_id),
                    LogSource::System,
                    format!("Synced {} edge functions", func_count),
                );
                state.add_log(log.clone()).await;
                app_handle.emit("log", &log).ok();
            }
        },
        Err(e) => {
             let log = LogEntry::error(Some(project_id), LogSource::System, format!("Failed to list functions: {}", e));
             state.add_log(log).await;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn push_project(
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

    let log = LogEntry::info(Some(uuid), LogSource::System, "Pushing schema changes...".to_string());
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    // 1. Introspect Remote
    let introspector = crate::introspection::Introspector::new(&api, project_ref.clone());
    let remote_schema = introspector.introspect().await.map_err(|e| e.to_string())?;

    // 2. Parse Local - try multiple schema paths
    let schema_paths = [
        std::path::Path::new(&project.local_path).join("supabase/schemas/schema.sql"),
        std::path::Path::new(&project.local_path).join("supabase/schema.sql"),
    ];
    
    let schema_path = schema_paths.iter().find(|p| p.exists());
    
    let schema_path = match schema_path {
        Some(p) => p.clone(),
        None => {
            return Err("Schema file not found (checked supabase/schemas/schema.sql and supabase/schema.sql)".to_string());
        }
    };
    let local_sql = tokio::fs::read_to_string(&schema_path).await.map_err(|e| e.to_string())?;
    let local_schema = crate::parsing::parse_schema_sql(&local_sql).map_err(|e| e.to_string())?;

    // 3. Diff (Remote -> Local)
    let diff = crate::diff::compute_diff(&remote_schema, &local_schema);

    let summary = diff.summarize();
    let log = LogEntry::info(
        Some(uuid),
        LogSource::System,
        format!("Diff Summary:\n{}", summary),
    );
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    // 4. Generate Migration SQL
    let migration_sql = crate::generator::generate_sql(&diff, &local_schema);

    if migration_sql.trim().is_empty() {
         let log = LogEntry::success(
            Some(uuid),
            LogSource::System,
            "No schema changes detected.".to_string(),
        );
        state.add_log(log.clone()).await;
        app_handle.emit("log", &log).ok();
        return Ok("No changes".to_string());
    }

    let log = LogEntry::info(
        Some(uuid),
        LogSource::System,
        format!("Applying changes:\n{}", migration_sql),
    );
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    // 5. Execute
    let result = api.run_query(&project_ref, &migration_sql, false).await.map_err(|e| e.to_string())?;

    if let Some(err) = result.error {
        let log = LogEntry::error(Some(uuid), LogSource::System, format!("Migration failed: {}", err));
        state.add_log(log.clone()).await;
        app_handle.emit("log", &log).ok();
        return Err(err);
    }

    let log = LogEntry::success(
        Some(uuid),
        LogSource::System,
        "Schema changes pushed successfully.".to_string(),
    );
    state.add_log(log.clone()).await;
    app_handle.emit("log", &log).ok();

    Ok(migration_sql)
}

// Project commands

#[tauri::command]
pub async fn create_project(
    app_handle: tauri::AppHandle,
    name: String,
    local_path: String,
    supabase_project_id: Option<String>,
    supabase_project_ref: Option<String>,
    organization_id: Option<String>,
) -> Result<Project, String> {
    let state = app_handle.state::<Arc<AppState>>();

    let (project_id, project_ref) = if let Some(refer) = supabase_project_ref {
        // Sync/Link Mode
        let log = LogEntry::info(
            None, 
            LogSource::System, 
            format!("Linking to existing project: {}", refer)
        );
        state.add_log(log.clone()).await;
        app_handle.emit("log", &log).ok();

        // Resolve Project ID (if missing)
        let pid = if let Some(id) = supabase_project_id {
            id
        } else {
            // Try to fetch from API to get the canonical ID
             if state.has_access_token().await {
                 match state.get_api_client().await {
                     Ok(api) => {
                         match api.get_project(&refer).await {
                             Ok(p) => p.id,
                             Err(_) => refer.clone() // Fallback
                         }
                     },
                     Err(_) => refer.clone() // Fallback
                 }
             } else {
                 refer.clone() // Fallback
             }
        };

        // Auto-pull schema logic
        println!("[DEBUG] Starting auto-pull schema logic");
        let supabase_dir = std::path::Path::new(&local_path).join("supabase");
        let schemas_dir = supabase_dir.join("schemas");
        let schema_path = schemas_dir.join("schema.sql");
        
        println!("[DEBUG] Checking if schema exists at: {:?}", schema_path);
        if !schema_path.exists() {
             println!("[DEBUG] Schema does not exist, checking access token...");
             if state.has_access_token().await {
                println!("[DEBUG] Has access token, getting API client...");
                if let Ok(api) = state.get_api_client().await {
                   println!("[DEBUG] Got API client, creating introspector...");
                   let introspector = crate::introspection::Introspector::new(&api, refer.clone());
                   
                   let log = LogEntry::info(
                        None, 
                        LogSource::System, 
                        format!("Auto-pulling schema for linked project: {}", refer)
                    );
                   state.add_log(log.clone()).await;
                   app_handle.emit("log", &log).ok();

                   println!("[DEBUG] Starting introspection...");
                   match introspector.introspect().await {
                        Ok(remote_schema) => {
                             println!("[DEBUG] Introspection successful, generating SQL...");
                             let empty_schema = crate::schema::DbSchema::new();
                             let diff = crate::diff::compute_diff(&empty_schema, &remote_schema);
                             let sql = crate::generator::generate_sql(&diff, &remote_schema);
                             
                             println!("[DEBUG] Creating schema dir: {:?}", schemas_dir);
                             if !schemas_dir.exists() {
                                tokio::fs::create_dir_all(&schemas_dir).await.map_err(|e| e.to_string())?;
                             }
                             println!("[DEBUG] Writing schema.sql ({} bytes)", sql.len());
                             tokio::fs::write(&schema_path, &sql).await.map_err(|e| e.to_string())?;

                             let log = LogEntry::success(
                                None,
                                LogSource::System,
                                "Schema pulled successfully".to_string(),
                            );
                            state.add_log(log.clone()).await;
                            app_handle.emit("log", &log).ok();
                        },
                        Err(e) => {
                             println!("[DEBUG] Introspection failed: {}", e);
                             let log = LogEntry::error(
                                None, 
                                LogSource::System, 
                                format!("Failed to auto-pull schema: {}", e)
                            );
                            state.add_log(log.clone()).await;
                            app_handle.emit("log", &log).ok();
                        }
                   }

                   // Auto-pull Edge Functions
                   println!("[DEBUG] Starting function sync...");
                   match api.list_functions(&refer).await {
                       Ok(funcs) => {
                           println!("[DEBUG] Listed {} functions", funcs.len());
                           let functions_dir = supabase_dir.join("functions");
                           if !functions_dir.exists() {
                               tokio::fs::create_dir_all(&functions_dir).await.ok();
                           }

                           let mut func_count = 0;
                           for func in funcs {
                               let func_dir = functions_dir.join(&func.slug);
                               if !func_dir.exists() {
                                   tokio::fs::create_dir_all(&func_dir).await.ok();
                               }
                               
                               match api.get_function_body(&refer, &func.slug).await {
                                   Ok(body) => {
                                       // Check content type to determine extension
                                       let (filename, is_binary) = if body.content_type == "application/vnd.denoland.eszip" || body.content_type == "application/octet-stream" {
                                           ("function.eszip", true)
                                       } else {
                                           ("index.ts", false)
                                       };
                                       
                                       if is_binary {
                                            // Attempt to unpack ESZIP
                                            println!("[DEBUG] Starting ESZIP unpack for {}", func.slug);
                                            let reader = futures::io::Cursor::new(body.data.clone());
                                            let buf_reader = futures::io::BufReader::new(reader);
                                            let mut unpacked = false;
                                            
                                            println!("[DEBUG] About to parse ESZIP...");
                                            // We use a block to capture the result of unpacking attempt
                                            if let Ok((eszip, loader)) = eszip::EszipV2::parse(buf_reader).await {
                                                println!("[DEBUG] ESZIP parsed successfully");
                                                // Spawn the loader to drive it in the background
                                                // The loader must run for module.source() to return data
                                                let loader_handle = tokio::spawn(async move {
                                                    println!("[DEBUG] Loader task started");
                                                    let _ = loader.await;
                                                    println!("[DEBUG] Loader task completed");
                                                });

                                                let specifiers = eszip.specifiers();
                                                println!("[DEBUG] Found {} specifiers", specifiers.len());
                                                for specifier in specifiers {
                                                    // Filter for local files (usually starting with file:///)
                                                    // and ignore remote dependencies (http/https usually, but eszip might bundle them)
                                                    if specifier.starts_with("file:///") {
                                                        println!("[DEBUG] Processing specifier: {}", specifier);
                                                        if let Some(module) = eszip.get_module(&specifier) {
                                                            println!("[DEBUG] Got module, awaiting source...");
                                                            if let Some(source) = module.source().await {
                                                                println!("[DEBUG] Got source, {} bytes", source.len());
                                                                // Extract filename from specifier
                                                                // e.g. file:///src/index.ts -> index.ts
                                                                // e.g. file:///src/utils/helper.ts -> utils/helper.ts
                                                                let path_str = specifier.trim_start_matches("file:///");
                                                                
                                                                let output_path = if path_str.ends_with("/index.ts") || path_str == "index.ts" {
                                                                     Some("index.ts")
                                                                } else if !path_str.contains("node_modules") && !path_str.contains("deno_dir") {
                                                                     std::path::Path::new(path_str).file_name().and_then(|n| n.to_str())
                                                                } else {
                                                                     None
                                                                };

                                                                if let Some(out_name) = output_path {
                                                                     let _ = tokio::fs::write(func_dir.join(out_name), source).await;
                                                                     unpacked = true;
                                                                }
                                                            } else {
                                                                println!("[DEBUG] module.source() returned None");
                                                            }
                                                        }
                                                    }
                                                }
                                                
                                                println!("[DEBUG] Finished processing specifiers, aborting loader");
                                                // Abort the loader if we're done
                                                loader_handle.abort();
                                            } else {
                                                println!("[DEBUG] ESZIP parse failed");
                                            }

                                            if !unpacked {
                                                 // Fallback: save eszip
                                                 let _ = tokio::fs::write(func_dir.join("function.eszip"), &body.data).await;
                                                 let notice = format!("// The source code for function '{}' could not be unpacked.\n// The deployed bundle has been downloaded as 'function.eszip'.", func.slug);
                                                 let _ = tokio::fs::write(func_dir.join("index.ts"), notice).await;
                                            }
                                       } else {
                                            // Plain text
                                            let file_path = func_dir.join("index.ts");
                                            if let Err(e) = tokio::fs::write(&file_path, &body.data).await {
                                                 let log = LogEntry::error(None, LogSource::System, format!("Failed to save function {}: {}", func.slug, e));
                                                 state.add_log(log).await;
                                            }
                                       }
                                       func_count += 1;
                                   },
                                   Err(e) => {
                                        let log = LogEntry::error(None, LogSource::System, format!("Failed to download function {}: {}", func.slug, e));
                                        state.add_log(log).await;
                                   }
                               }
                           }
                           
                           if func_count > 0 {
                               let log = LogEntry::success(
                                   None,
                                   LogSource::System,
                                   format!("Synced {} edge functions", func_count),
                               );
                               state.add_log(log.clone()).await;
                               app_handle.emit("log", &log).ok();
                           }
                       },
                       Err(e) => {
                            let log = LogEntry::error(None, LogSource::System, format!("Failed to list functions: {}", e));
                            state.add_log(log).await;
                       }
                   }
                }
             }
        }
        
        (Some(pid), Some(refer))
    } else {
        // Create Mode - Try to create remote project if authenticated
        if state.has_access_token().await {
            let api = state.get_api_client().await.map_err(|e| e.to_string())?;
            
            // Get organizations
            let orgs = api.list_organizations().await.map_err(|e| format!("Failed to list organizations: {}", e))?;
            
            let org = if let Some(oid) = &organization_id {
                orgs.iter().find(|o| o.id == *oid).ok_or("Selected organization not found.".to_string())?
            } else {
                orgs.first().ok_or("No organizations found. Please create one in Supabase dashboard.".to_string())?
            };

            // Generate a secure password (using UUID v4 for now as it's random enough)
            let db_pass = Uuid::new_v4().to_string();
            let region = "us-east-1"; // Default region

            let log = LogEntry::info(
                None,
                LogSource::System,
                format!("Creating remote Supabase project '{}'...", name),
            );
            state.add_log(log.clone()).await;
            app_handle.emit("log", &log).ok();

            match api.create_project(&name, &org.id, &db_pass, region).await {
                Ok(remote_project) => {
                    let log = LogEntry::success(
                        None,
                        LogSource::System,
                        format!("Created remote project: {}", remote_project.name),
                    );
                    state.add_log(log.clone()).await;
                    app_handle.emit("log", &log).ok();

                    // Supabase API 'id' is the project reference (e.g., abcdefghi)
                    (Some(remote_project.id.clone()), Some(remote_project.id)) 
                },
                Err(e) => {
                    let log = LogEntry::error(
                        None,
                        LogSource::System,
                        format!("Failed to create remote project: {}", e),
                    );
                    state.add_log(log.clone()).await;
                    app_handle.emit("log", &log).ok();
                    return Err(format!("Failed to create remote project: {}", e));
                }
            }
        } else {
            (None, None)
        }
    };

    let project = if let (Some(pid), Some(pref)) = (project_id, project_ref) {
        Project::with_remote(name, local_path, pid, pref)
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

    watcher::start_watching(&app_handle, uuid, &project.local_path).await
}

#[tauri::command]
pub async fn stop_watching(app_handle: tauri::AppHandle, project_id: String) -> Result<(), String> {
    let uuid = Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;
    watcher::stop_watching(&app_handle, uuid).await
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

// Supabase Logs API commands

#[tauri::command]
pub async fn query_supabase_logs(
    app_handle: tauri::AppHandle,
    project_id: String,
    sql: Option<String>,
    iso_timestamp_start: Option<String>,
    iso_timestamp_end: Option<String>,
) -> Result<serde_json::Value, String> {
    let state = app_handle.state::<Arc<AppState>>();
    let uuid = Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;

    let project = state.get_project(uuid).await.map_err(|e| e.to_string())?;
    let project_ref = project
        .supabase_project_ref
        .ok_or("Project not linked to Supabase")?;

    let api = state.get_api_client().await.map_err(|e| e.to_string())?;

    api.query_logs(
        &project_ref,
        sql.as_deref(),
        iso_timestamp_start.as_deref(),
        iso_timestamp_end.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_edge_function_logs(
    app_handle: tauri::AppHandle,
    project_id: String,
    function_name: Option<String>,
    minutes: Option<u32>,
) -> Result<serde_json::Value, String> {
    let state = app_handle.state::<Arc<AppState>>();
    let uuid = Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;

    let project = state.get_project(uuid).await.map_err(|e| e.to_string())?;
    let project_ref = project
        .supabase_project_ref
        .ok_or("Project not linked to Supabase")?;

    let api = state.get_api_client().await.map_err(|e| e.to_string())?;

    api.get_edge_function_logs(&project_ref, function_name.as_deref(), minutes.unwrap_or(60))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_postgres_logs(
    app_handle: tauri::AppHandle,
    project_id: String,
    minutes: Option<u32>,
) -> Result<serde_json::Value, String> {
    let state = app_handle.state::<Arc<AppState>>();
    let uuid = Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;

    let project = state.get_project(uuid).await.map_err(|e| e.to_string())?;
    let project_ref = project
        .supabase_project_ref
        .ok_or("Project not linked to Supabase")?;

    let api = state.get_api_client().await.map_err(|e| e.to_string())?;

    api.get_postgres_logs(&project_ref, minutes.unwrap_or(60))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_auth_logs(
    app_handle: tauri::AppHandle,
    project_id: String,
    minutes: Option<u32>,
) -> Result<serde_json::Value, String> {
    let state = app_handle.state::<Arc<AppState>>();
    let uuid = Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;

    let project = state.get_project(uuid).await.map_err(|e| e.to_string())?;
    let project_ref = project
        .supabase_project_ref
        .ok_or("Project not linked to Supabase")?;

    let api = state.get_api_client().await.map_err(|e| e.to_string())?;

    api.get_auth_logs(&project_ref, minutes.unwrap_or(60))
        .await
        .map_err(|e| e.to_string())
}
