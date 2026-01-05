use crate::schema::{
    ColumnInfo, DbSchema, EnumInfo, ForeignKeyInfo, FunctionArg, FunctionInfo, IndexInfo,
    PolicyInfo, TableInfo, TriggerInfo,
};
use crate::supabase_api::SupabaseApi;
use serde::Deserialize;
use std::collections::HashMap;

pub struct Introspector<'a> {
    api: &'a SupabaseApi,
    project_ref: String,
}

impl<'a> Introspector<'a> {
    pub fn new(api: &'a SupabaseApi, project_ref: String) -> Self {
        Self { api, project_ref }
    }

    pub async fn introspect(&self) -> Result<DbSchema, String> {
        println!("[DEBUG introspect] Starting introspection for project: {}", self.project_ref);
        
        // Run all bulk queries in parallel for maximum efficiency
        println!("[DEBUG introspect] Running bulk queries...");
        
        let (enums, functions, tables_data) = tokio::try_join!(
            self.get_enums(),
            self.get_functions(),
            self.get_all_tables_bulk()
        )?;
        
        
        let total_triggers: usize = tables_data.values().map(|t| t.triggers.len()).sum();
        let total_policies: usize = tables_data.values().map(|t| t.policies.len()).sum();

        println!("[DEBUG introspect] Got {} enums, {} functions, {} tables", 
            enums.len(), functions.len(), tables_data.len());
        println!("[DEBUG introspect] Got {} triggers, {} policies", total_triggers, total_policies);

        println!("[DEBUG introspect] Introspection complete!");
        Ok(DbSchema {
            tables: tables_data,
            enums,
            functions,
        })
    }

    async fn get_enums(&self) -> Result<HashMap<String, EnumInfo>, String> {
        let query = r#"
            SELECT t.typname as name, array_agg(e.enumlabel ORDER BY e.enumsortorder) as values
            FROM pg_type t
            JOIN pg_enum e ON t.oid = e.enumtypid
            JOIN pg_namespace n ON t.typnamespace = n.oid
            WHERE n.nspname = 'public'
            GROUP BY t.typname
        "#;

        #[derive(Deserialize)]
        struct Row {
            name: String,
            values: serde_json::Value,
        }

        let result = self
            .api
            .run_query(&self.project_ref, query, true)
            .await
            .map_err(|e| e.to_string())?;

        let rows: Vec<Row> =
            serde_json::from_value(result.result.unwrap_or(serde_json::Value::Array(vec![])))
                .map_err(|e| e.to_string())?;

        let mut enums = HashMap::new();
        for row in rows {
            let values = parse_pg_array(&row.values);
            enums.insert(
                row.name.clone(),
                EnumInfo {
                    name: row.name,
                    values,
                },
            );
        }

        Ok(enums)
    }

    async fn get_functions(&self) -> Result<HashMap<String, FunctionInfo>, String> {
        let query = r#"
            SELECT
              p.proname as name,
              pg_get_function_result(p.oid) as return_type,
              pg_get_function_arguments(p.oid) as args,
              l.lanname as language,
              p.prosrc as definition
            FROM pg_proc p
            JOIN pg_language l ON p.prolang = l.oid
            JOIN pg_namespace n ON p.pronamespace = n.oid
            WHERE n.nspname = 'public'
        "#;

        #[derive(Deserialize)]
        struct Row {
            name: String,
            return_type: String,
            args: String,
            language: String,
            definition: String,
        }

        let result = self
            .api
            .run_query(&self.project_ref, query, true)
            .await
            .map_err(|e| e.to_string())?;

        let rows: Vec<Row> =
            serde_json::from_value(result.result.unwrap_or(serde_json::Value::Array(vec![])))
                .map_err(|e| e.to_string())?;

        let mut functions = HashMap::new();
        for row in rows {
            functions.insert(
                row.name.clone(),
                FunctionInfo {
                    name: row.name,
                    args: parse_function_args(&row.args),
                    return_type: row.return_type,
                    language: row.language,
                    definition: row.definition,
                },
            );
        }

        Ok(functions)
    }

    /// Fetch all table information using bulk queries (minimal API calls)
    async fn get_all_tables_bulk(&self) -> Result<HashMap<String, TableInfo>, String> {
        // Single comprehensive query that gets tables + columns + constraints
        let bulk_query = r#"
            WITH table_list AS (
                SELECT table_name
                FROM information_schema.tables
                WHERE table_schema = 'public'
                AND table_type = 'BASE TABLE'
            ),
            columns_data AS (
                SELECT
                    t.relname as table_name,
                    a.attname as column_name,
                    format_type(a.atttypid, a.atttypmod) as data_type,
                    CASE WHEN a.attnotnull THEN 'NO' ELSE 'YES' END as is_nullable,
                    pg_get_expr(d.adbin, d.adrelid) as column_default,
                    t_type.typname as udt_name,
                    CASE WHEN a.attidentity != '' THEN 'YES' ELSE 'NO' END as is_identity,
                    COALESCE(pk.is_primary, false) as is_primary_key,
                    false as is_unique -- Simplified, unique handled by indexes usually
                FROM pg_attribute a
                JOIN pg_class t ON a.attrelid = t.oid
                JOIN pg_namespace n ON t.relnamespace = n.oid
                JOIN pg_type t_type ON a.atttypid = t_type.oid
                LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
                LEFT JOIN (
                    SELECT c.conrelid, unnest(c.conkey) as attnum, true as is_primary
                    FROM pg_constraint c
                    WHERE c.contype = 'p'
                ) pk ON pk.conrelid = a.attrelid AND pk.attnum = a.attnum
                WHERE n.nspname = 'public'
                AND t.relkind = 'r'
                AND a.attnum > 0
                AND NOT a.attisdropped
            ),
            fk_data AS (
                SELECT
                    c.relname as table_name,
                    con.conname as constraint_name,
                    a.attname as column_name,
                    cf.relname as foreign_table,
                    af.attname as foreign_column,
                    CASE con.confdeltype
                        WHEN 'a' THEN 'NO ACTION'
                        WHEN 'r' THEN 'RESTRICT'
                        WHEN 'c' THEN 'CASCADE'
                        WHEN 'n' THEN 'SET NULL'
                        WHEN 'd' THEN 'SET DEFAULT'
                        ELSE 'NO ACTION'
                    END as on_delete
                FROM pg_constraint con
                JOIN pg_class c ON con.conrelid = c.oid
                JOIN pg_namespace n ON c.relnamespace = n.oid
                JOIN pg_class cf ON con.confrelid = cf.oid
                CROSS JOIN LATERAL unnest(con.conkey, con.confkey) AS k(con_attnum, conf_attnum)
                JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = k.con_attnum
                JOIN pg_attribute af ON af.attrelid = cf.oid AND af.attnum = k.conf_attnum
                WHERE n.nspname = 'public'
                AND con.contype = 'f'
            ),
            index_data AS (
                SELECT
                    t.relname as table_name,
                    i.relname as index_name,
                    array_agg(a.attname ORDER BY array_position(ix.indkey, a.attnum)) as columns,
                    ix.indisunique as is_unique,
                    ix.indisprimary as is_primary,
                    MAX(con.conname) as owning_constraint
                FROM pg_class t
                JOIN pg_index ix ON t.oid = ix.indrelid
                JOIN pg_class i ON i.oid = ix.indexrelid
                JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey)
                JOIN pg_namespace n ON t.relnamespace = n.oid
                LEFT JOIN pg_constraint con ON con.conindid = i.oid
                WHERE n.nspname = 'public'
                AND NOT ix.indisprimary
                GROUP BY t.relname, i.relname, ix.indisunique, ix.indisprimary
            ),
            trigger_data AS (
                SELECT
                    c.relname as table_name,
                    t.tgname as trigger_name,
                    t.tgtype::integer as tgtype,
                    p.proname as function_name
                FROM pg_trigger t
                JOIN pg_class c ON c.oid = t.tgrelid
                JOIN pg_namespace n ON n.oid = c.relnamespace
                JOIN pg_proc p ON p.oid = t.tgfoid
                WHERE n.nspname = 'public'
                AND NOT t.tgisinternal
            ),
            policy_data AS (
                SELECT
                    c.relname as table_name,
                    p.polname as name,
                    p.polcmd as cmd,
                    p.polroles as roles,
                    pg_get_expr(p.polqual, p.polrelid) as qual,
                    pg_get_expr(p.polwithcheck, p.polrelid) as with_check
                FROM pg_policy p
                JOIN pg_class c ON c.oid = p.polrelid
                JOIN pg_namespace n ON n.oid = c.relnamespace
                WHERE n.nspname = 'public'
            ),
            rls_data AS (
                SELECT c.relname as table_name, c.relrowsecurity as rls_enabled
                FROM pg_class c
                JOIN pg_namespace n ON n.oid = c.relnamespace
                WHERE n.nspname = 'public' AND c.relkind = 'r'
            )
            SELECT json_build_object(
                'tables', (SELECT json_agg(table_name) FROM table_list),
                'columns', (SELECT json_agg(row_to_json(columns_data)) FROM columns_data),
                'foreign_keys', (SELECT json_agg(row_to_json(fk_data)) FROM fk_data),
                'indexes', (SELECT json_agg(row_to_json(index_data)) FROM index_data),
                'triggers', (SELECT json_agg(row_to_json(trigger_data)) FROM trigger_data),
                'policies', (SELECT json_agg(row_to_json(policy_data)) FROM policy_data),
                'rls', (SELECT json_agg(row_to_json(rls_data)) FROM rls_data)
            ) as data
        "#;

        let result = self
            .api
            .run_query(&self.project_ref, bulk_query, true)
            .await
            .map_err(|e| format!("Bulk query failed: {}", e))?;

        let rows: Vec<serde_json::Value> =
            serde_json::from_value(result.result.unwrap_or(serde_json::Value::Array(vec![])))
                .map_err(|e| e.to_string())?;

        let data = rows.first()
            .and_then(|r| r.get("data"))
            .cloned()
            .unwrap_or(serde_json::json!({}));

        // Parse the bulk response
        self.parse_bulk_response(&data)
    }

    fn parse_bulk_response(&self, data: &serde_json::Value) -> Result<HashMap<String, TableInfo>, String> {
        // Extract table names
        let table_names: Vec<String> = data.get("tables")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        // Parse columns
        #[derive(Deserialize)]
        struct ColumnRow {
            table_name: String,
            column_name: String,
            data_type: String,
            is_nullable: String,
            column_default: Option<String>,
            udt_name: String,
            is_identity: String,
            is_primary_key: bool,
            is_unique: bool,
        }
        let columns: Vec<ColumnRow> = data.get("columns")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        // Parse foreign keys
        #[derive(Deserialize)]
        struct FkRow {
            table_name: String,
            constraint_name: String,
            column_name: String,
            foreign_table: String,
            foreign_column: String,
            on_delete: String,
        }
        let fks: Vec<FkRow> = data.get("foreign_keys")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        // Parse indexes
        #[derive(Deserialize)]
        struct IndexRow {
            table_name: String,
            index_name: String,
            columns: serde_json::Value,
            is_unique: bool,
            is_primary: bool,
            owning_constraint: Option<String>,
        }
        let indexes: Vec<IndexRow> = data.get("indexes")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        // Parse triggers
        #[derive(Deserialize)]
        struct TriggerRow {
            table_name: String,
            trigger_name: String,
            tgtype: i32,
            function_name: String,
        }
        let triggers: Vec<TriggerRow> = data.get("triggers")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        // Parse policies
        #[derive(Deserialize)]
        struct PolicyRow {
            table_name: String,
            name: String,
            cmd: String,
            roles: serde_json::Value,
            qual: Option<String>,
            with_check: Option<String>,
        }
        let policies: Vec<PolicyRow> = data.get("policies")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        // Parse RLS
        #[derive(Deserialize)]
        struct RlsRow {
            table_name: String,
            rls_enabled: bool,
        }
        let rls_data: Vec<RlsRow> = data.get("rls")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        // Build tables map
        let mut tables: HashMap<String, TableInfo> = HashMap::new();

        // Initialize all tables
        for table_name in table_names {
            tables.insert(table_name.clone(), TableInfo {
                table_name,
                columns: HashMap::new(),
                foreign_keys: vec![],
                indexes: vec![],
                triggers: vec![],
                rls_enabled: false,
                policies: vec![],
            });
        }

        // Populate columns
        for col in columns {
            if let Some(table) = tables.get_mut(&col.table_name) {
                let mut final_data_type = col.data_type.clone();
                if final_data_type == "ARRAY" {
                     if col.udt_name.starts_with('_') {
                         final_data_type = format!("{}[]", &col.udt_name[1..]);
                     }
                }

                table.columns.insert(
                    col.column_name.clone(),
                    ColumnInfo {
                        column_name: col.column_name,
                        data_type: final_data_type,
                        is_nullable: col.is_nullable == "YES",
                        column_default: col.column_default,
                        udt_name: col.udt_name.clone(),
                        is_primary_key: col.is_primary_key,
                        is_unique: col.is_unique,
                        is_identity: col.is_identity == "YES",
                        enum_name: None,
                        is_array: col.udt_name.starts_with("_"),
                    },
                );
            }
        }

        // Populate foreign keys
        for fk in fks {
            if let Some(table) = tables.get_mut(&fk.table_name) {
                table.foreign_keys.push(ForeignKeyInfo {
                    constraint_name: fk.constraint_name,
                    column_name: fk.column_name,
                    foreign_table: fk.foreign_table,
                    foreign_column: fk.foreign_column,
                    on_delete: fk.on_delete,
                });
            }
        }

        // Populate indexes
        println!("[DEBUG] Indexes fetched from DB: {}", indexes.len());
        for idx in indexes {
            println!("[DEBUG] Adding index {} to table {}, columns: {:?}", idx.index_name, idx.table_name, idx.columns);
            if let Some(table) = tables.get_mut(&idx.table_name) {
                table.indexes.push(IndexInfo {
                    index_name: idx.index_name,
                    columns: parse_pg_array(&idx.columns),
                    is_unique: idx.is_unique,
                    is_primary: idx.is_primary,
                    owning_constraint: idx.owning_constraint,
                });
            }
        }

        // Populate triggers (consolidate by trigger name)
        // Populate triggers (consolidate by trigger name)
        let mut trigger_map: HashMap<(String, String), TriggerInfo> = HashMap::new();
        for trig in triggers {
            let key = (trig.table_name.clone(), trig.trigger_name.clone());
            let entry = trigger_map.entry(key).or_insert_with(|| {
                // Decode tgtype for timing and orientation (once)
                let timing = if trig.tgtype & 2 != 0 { "BEFORE" } else { "AFTER" };
                let orientation = if trig.tgtype & 1 != 0 { "ROW" } else { "STATEMENT" };
                
                TriggerInfo {
                    name: trig.trigger_name.clone(),
                    events: vec![],
                    timing: timing.to_string(),
                    orientation: orientation.to_string(),
                    function_name: trig.function_name.clone(),
                }
            });

            // Decode tgtype for events (bitmask)
            if trig.tgtype & 4 != 0 { entry.events.push("INSERT".to_string()); }
            if trig.tgtype & 8 != 0 { entry.events.push("DELETE".to_string()); }
            if trig.tgtype & 16 != 0 { entry.events.push("UPDATE".to_string()); }
            if trig.tgtype & 32 != 0 { entry.events.push("TRUNCATE".to_string()); }
        }
        
        for ((table_name, _), trigger) in trigger_map {
            if let Some(table) = tables.get_mut(&table_name) {
                // Deduplicate events just in case (though bits are unique)
                let mut final_trigger = trigger.clone();
                final_trigger.events.dedup();
                table.triggers.push(final_trigger);
            }
        }

        // Populate policies
        for pol in policies {
            if let Some(table) = tables.get_mut(&pol.table_name) {
                table.policies.push(PolicyInfo {
                    name: pol.name,
                    cmd: parse_policy_cmd(&pol.cmd),
                    roles: parse_pg_oid_array(&pol.roles),
                    qual: pol.qual,
                    with_check: pol.with_check,
                });
            }
        }

        // Populate RLS status
        for rls in rls_data {
            if let Some(table) = tables.get_mut(&rls.table_name) {
                table.rls_enabled = rls.rls_enabled;
            }
        }

        Ok(tables)
    }
}

// Helpers

fn parse_pg_array(val: &serde_json::Value) -> Vec<String> {
    if let Some(arr) = val.as_array() {
        arr.iter()
            .map(|v| v.as_str().unwrap_or("").to_string())
            .collect()
    } else if let Some(s) = val.as_str() {
        // Handle "{a,b}" string
        s.trim_matches(|c| c == '{' || c == '}')
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        vec![]
    }
}

fn parse_pg_oid_array(val: &serde_json::Value) -> Vec<String> {
    // Simplified: return "public" if {0} or equivalent, else "authenticated" placeholder
    let s = val.to_string();
    if s.contains("{0}") {
        vec!["public".to_string()]
    } else {
        vec!["authenticated".to_string()] // Placeholder until we map OIDs
    }
}

fn parse_policy_cmd(cmd: &str) -> String {
    match cmd {
        "r" => "SELECT".to_string(),
        "a" => "INSERT".to_string(),
        "w" => "UPDATE".to_string(),
        "d" => "DELETE".to_string(),
        "*" => "ALL".to_string(),
        _ => cmd.to_string(),
    }
}

fn parse_function_args(args_str: &str) -> Vec<FunctionArg> {
    if args_str.is_empty() {
        return vec![];
    }
    args_str
        .split(',')
        .map(|s| {
            let parts: Vec<&str> = s.trim().split_whitespace().collect();
            if parts.len() >= 2 {
                FunctionArg {
                    name: parts[0].to_string(),
                    type_: parts[1..].join(" "),
                }
            } else {
                FunctionArg {
                    name: "".to_string(),
                    type_: s.trim().to_string(),
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_bulk_response_array_type() {
        let api = SupabaseApi::new("token".to_string());
        let introspector = Introspector::new(&api, "project".to_string());

        let data = json!({
            "tables": ["test_table"],
            "columns": [
                {
                    "table_name": "test_table",
                    "column_name": "tags",
                    "data_type": "ARRAY",
                    "is_nullable": "NO",
                    "column_default": null,
                    "udt_name": "_text",
                    "is_identity": "NO",
                    "is_primary_key": false,
                    "is_unique": false
                }
            ],
            "foreign_keys": [],
            "indexes": [],
            "triggers": [],
            "policies": [],
            "rls": []
        });

        let result = introspector.parse_bulk_response(&data).unwrap();
        let table = result.get("test_table").unwrap();
        let col = table.columns.get("tags").unwrap();
        
        assert_eq!(col.data_type, "text[]");
    }

    #[test]
    fn test_parse_bulk_response_pg_trigger() {
        let api = SupabaseApi::new("token".to_string());
        let introspector = Introspector::new(&api, "project".to_string());

        let data = json!({
            "tables": ["test_table"],
            "columns": [],
            "foreign_keys": [],
            "indexes": [],
            "triggers": [
                {
                    "table_name": "test_table",
                    "trigger_name": "test_trigger",
                    "tgtype": 21, // 16 (UPDATE) + 4 (INSERT) + 1 (ROW) = 21. No 2 bit => AFTER.
                    "function_name": "test_func"
                }
            ],
            "policies": [],
            "rls": []
        });

        let result = introspector.parse_bulk_response(&data).unwrap();
        let table = result.get("test_table").unwrap();
        let trigger = &table.triggers[0];

        assert_eq!(trigger.name, "test_trigger");
        assert_eq!(trigger.orientation, "ROW");
        assert_eq!(trigger.timing, "AFTER");
        assert!(trigger.events.contains(&"UPDATE".to_string()));
        assert!(trigger.events.contains(&"INSERT".to_string()));
        assert_eq!(trigger.function_name, "test_func");
    }
}
