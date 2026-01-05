use crate::diff::{EnumChangeType, SchemaDiff};
use crate::schema::DbSchema;

pub fn generate_sql(diff: &SchemaDiff, local_schema: &DbSchema) -> String {
    let mut statements = vec![];

    // Enums
    for enum_change in &diff.enum_changes {
        if enum_change.type_ == EnumChangeType::Create {
            if let Some(values) = &enum_change.values {
                let values_sql = values
                    .iter()
                    .map(|v| format!("'{}'", v))
                    .collect::<Vec<_>>()
                    .join(", ");
                statements.push(format!(
                    "CREATE TYPE {} AS ENUM ({});",
                    enum_change.name, values_sql
                ));
            }
        }
    }

    // Functions (Create / Update)
    // Helper to generate CREATE FUNCTION sql
    let gen_func_sql = |f: &crate::schema::FunctionInfo| -> String {
        let args_sql = f.args.iter()
            .map(|a| format!("{} {}", a.name, a.type_))
            .collect::<Vec<_>>()
            .join(", ");
        
        format!(
            "CREATE OR REPLACE FUNCTION {}({}) RETURNS {} LANGUAGE {} AS $${}$$;",
            f.name, args_sql, f.return_type, f.language, f.definition
        )
    };

    for func in &diff.functions_to_create {
        statements.push(gen_func_sql(func));
    }
    for func in &diff.functions_to_update {
        statements.push(gen_func_sql(func));
    }

    // CREATE Tables
    for table_name in &diff.tables_to_create {
        if let Some(table) = local_schema.tables.get(table_name) {
            let mut cols = vec![];
            
            // Sort columns for deterministic output
            let mut sorted_columns: Vec<_> = table.columns.values().collect();
            sorted_columns.sort_by(|a, b| a.column_name.cmp(&b.column_name));

            for col in sorted_columns {
                let mut def = format!("{} {}", col.column_name, col.data_type);
                if !col.is_nullable {
                    def.push_str(" NOT NULL");
                }
                if let Some(default) = &col.column_default {
                    def.push_str(&format!(" DEFAULT {}", default));
                }
                cols.push(def);
            }

            // PKs (Table Constraint)
            // Identify PK columns
            let mut pk_cols: Vec<String> = table.columns.values()
                .filter(|c| c.is_primary_key)
                .map(|c| c.column_name.clone())
                .collect();
            pk_cols.sort(); // Ensure deterministic order for composite keys mostly
            
            println!("[DEBUG] Table {}: PK columns found: {:?}", table_name, pk_cols);
            
            if !pk_cols.is_empty() {
                cols.push(format!("PRIMARY KEY ({})", pk_cols.join(", ")));
            }

            statements.push(format!(
                "CREATE TABLE {} (\n  {}\n);",
                table_name,
                cols.join(",\n  ")
            ));

            // Setup RLS if enabled
            if table.rls_enabled {
                statements.push(format!("ALTER TABLE {} ENABLE ROW LEVEL SECURITY;", table_name));
            }

            // Create Triggers
            for trigger in &table.triggers {
                let events = trigger.events.join(" OR ");
                statements.push(format!(
                    "CREATE TRIGGER {} {} {} ON {} FOR EACH {} EXECUTE FUNCTION {}();",
                    trigger.name,
                    trigger.timing,
                    events,
                    table_name,
                    trigger.orientation,
                    trigger.function_name
                ));
            }

            // Create Policies
            for policy in &table.policies {
                let roles = policy.roles.join(", ");
                let mut parts = vec![format!(
                    "CREATE POLICY \"{}\" ON {} FOR {} TO {}",
                    policy.name, table_name, policy.cmd, roles
                )];
                
                if let Some(qual) = &policy.qual {
                    parts.push(format!("USING ({})", qual));
                }
                if let Some(check) = &policy.with_check {
                    parts.push(format!("WITH CHECK ({})", check));
                }
                
                statements.push(format!("{};", parts.join(" ")));
            }
            
            // Create Indexes
            println!("[DEBUG] Table {} has {} indexes to create", table_name, table.indexes.len());
            for index in &table.indexes {
                 println!("[DEBUG] Generating index {}: is_primary={}, is_unique={}", index.index_name, index.is_primary, index.is_unique);
                 // Skip primary keys as they are usually handled by column definition or PK constraint
                 if !index.is_primary {
                     let unique = if index.is_unique { "UNIQUE" } else { "" };
                     let columns = index.columns.join(", ");
                     statements.push(format!(
                         "CREATE {} INDEX {} ON {} ({});",
                         unique, index.index_name, table_name, columns
                     ));
                 }
            }
        }
    }

    // ALTER Tables
    for (table_name, diff) in &diff.table_changes {
        // Add columns
        for col_name in &diff.columns_to_add {
            if let Some(table) = local_schema.tables.get(table_name) {
                if let Some(col) = table.columns.get(col_name) {
                    let mut def = format!("ADD COLUMN {} {}", col_name, col.data_type);
                    if !col.is_nullable {
                        // Adding NOT NULL column usually requires default or nullable first.
                        // Simplified: just add NOT NULL for now, user might handle default.
                        def.push_str(" NOT NULL");
                    }
                    if let Some(default) = &col.column_default {
                        def.push_str(&format!(" DEFAULT {}", default));
                    }
                    statements.push(format!("ALTER TABLE {} {};", table_name, def));
                }
            }
        }

        // Drop columns
        for col_name in &diff.columns_to_drop {
            statements.push(format!("ALTER TABLE {} DROP COLUMN {};", table_name, col_name));
        }

        // Modify columns
        for modification in &diff.columns_to_modify {
            if let Some((_, to_type)) = &modification.changes.type_change {
                statements.push(format!(
                    "ALTER TABLE {} ALTER COLUMN {} TYPE {};",
                    table_name, modification.column_name, to_type
                ));
            }
            if let Some((_, to_nullable)) = modification.changes.nullable_change {
                let action = if to_nullable {
                    "DROP NOT NULL"
                } else {
                    "SET NOT NULL"
                };
                statements.push(format!(
                    "ALTER TABLE {} ALTER COLUMN {} {};",
                    table_name, modification.column_name, action
                ));
            }
        }

        // RLS
        if let Some(enabled) = diff.rls_change {
            let action = if enabled { "ENABLE" } else { "DISABLE" };
            statements.push(format!("ALTER TABLE {} {} ROW LEVEL SECURITY;", table_name, action));
        }

        // Policies
        for p in &diff.policies_to_drop {
             statements.push(format!("DROP POLICY \"{}\" ON {};", p.name, table_name));
        }
        for p in &diff.policies_to_create {
            let roles = p.roles.join(", ");
            let mut parts = vec![format!(
                "CREATE POLICY \"{}\" ON {} FOR {} TO {}",
                p.name, table_name, p.cmd, roles
            )];
            if let Some(qual) = &p.qual {
                parts.push(format!("USING ({})", qual));
            }
            if let Some(check) = &p.with_check {
                parts.push(format!("WITH CHECK ({})", check));
            }
            statements.push(format!("{};", parts.join(" ")));
        }

        // Triggers
        for t in &diff.triggers_to_drop {
            statements.push(format!("DROP TRIGGER {} ON {};", t.name, table_name));
        }
        for t in &diff.triggers_to_create {
            let events = t.events.join(" OR ");
            statements.push(format!(
                "CREATE TRIGGER {} {} {} ON {} FOR EACH {} EXECUTE FUNCTION {}();",
                t.name, t.timing, events, table_name, t.orientation, t.function_name
            ));
        }

        // Indexes
        for i in &diff.indexes_to_drop {
             if let Some(constraint_name) = &i.owning_constraint {
                 statements.push(format!("ALTER TABLE {} DROP CONSTRAINT {};", table_name, constraint_name));
             } else {
                 statements.push(format!("DROP INDEX {};", i.index_name));
             }
        }
        for i in &diff.indexes_to_create {
             let unique = if i.is_unique { "UNIQUE" } else { "" };
             let columns = i.columns.join(", ");
             statements.push(format!(
                 "CREATE {} INDEX {} ON {} ({});",
                 unique, i.index_name, table_name, columns
             ));
        }
    }

    // DROP Tables
    for table_name in &diff.tables_to_drop {
        statements.push(format!("DROP TABLE {};", table_name));
    }

    // DROP Functions
    for func_name in &diff.functions_to_drop {
        statements.push(format!("DROP FUNCTION {};", func_name));
    }

    // CREATE FKs (deferred to end to avoid ordering issues)
    for table_name in &diff.tables_to_create {
        if let Some(table) = local_schema.tables.get(table_name) {
             for fk in &table.foreign_keys {
                statements.push(format!(
                    "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {}({}) ON DELETE {};",
                    table_name,
                    fk.constraint_name,
                    fk.column_name,
                    fk.foreign_table,
                    fk.foreign_column,
                    fk.on_delete
                ));
            }
        }
    }

    statements.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;
    use std::collections::HashMap;
    use crate::diff::*;

    #[test]
    fn test_generate_sql_full() {
        // Setup diff
        let mut diff = SchemaDiff {
            tables_to_create: vec![],
            tables_to_drop: vec![],
            table_changes: HashMap::new(),
            enum_changes: vec![],
            functions_to_create: vec![],
            functions_to_drop: vec!["old_func".to_string()],
            functions_to_update: vec![],
        };

        diff.functions_to_create.push(FunctionInfo {
            name: "new_func".to_string(),
            args: vec![FunctionArg { name: "a".to_string(), type_: "int".to_string() }],
            return_type: "void".to_string(),
            language: "plpgsql".to_string(),
            definition: "BEGIN END;".to_string(),
        });

        let mut table_diff = TableDiff {
            columns_to_add: vec![],
            columns_to_drop: vec![],
            columns_to_modify: vec![],
            rls_change: Some(true),
            policies_to_create: vec![],
            policies_to_drop: vec![],
            triggers_to_create: vec![],
            triggers_to_drop: vec![],
            indexes_to_create: vec![],
            indexes_to_drop: vec![],
        };
        
        table_diff.policies_to_create.push(PolicyInfo {
            name: "p1".to_string(),
            cmd: "SELECT".to_string(),
            roles: vec!["public".to_string()],
            qual: Some("true".to_string()),
            with_check: None,
        });

        table_diff.policies_to_drop.push(PolicyInfo {
            name: "p2 drop".to_string(),
            cmd: "".to_string(),
            roles: vec![],
            qual: None,
            with_check: None,
        });

        table_diff.triggers_to_create.push(TriggerInfo {
            name: "t1".to_string(),
            events: vec!["INSERT".to_string(), "UPDATE".to_string()],
            timing: "BEFORE".to_string(),
            orientation: "ROW".to_string(),
            function_name: "new_func".to_string(),
        });

        table_diff.indexes_to_create.push(IndexInfo {
            index_name: "idx_users_email".to_string(),
            columns: vec!["email".to_string()],
            is_unique: true,
            is_primary: false,
            owning_constraint: None,
        });

        table_diff.indexes_to_drop.push(IndexInfo {
            index_name: "idx_users_old".to_string(),
            columns: vec![],
            is_unique: false,
            is_primary: false,
            owning_constraint: None,
        });

        diff.table_changes.insert("users".to_string(), table_diff);

        // Run generator
        let schema = DbSchema::new(); // Dummy local schema, needed for Create Table lookups but not for alterations
        let sql = generate_sql(&diff, &schema);

        // Assert
        assert!(sql.contains("CREATE OR REPLACE FUNCTION new_func(a int) RETURNS void LANGUAGE plpgsql AS $$BEGIN END;$$;"));
        assert!(sql.contains("ALTER TABLE users ENABLE ROW LEVEL SECURITY;"));
        assert!(sql.contains("CREATE POLICY \"p1\" ON users FOR SELECT TO public USING (true);"));
        assert!(sql.contains("DROP POLICY \"p2 drop\" ON users;"));
        assert!(sql.contains("CREATE TRIGGER t1 BEFORE INSERT OR UPDATE ON users FOR EACH ROW EXECUTE FUNCTION new_func();"));
        assert!(sql.contains("CREATE UNIQUE INDEX idx_users_email ON users (email);"));
        assert!(sql.contains("DROP INDEX idx_users_old;"));
        assert!(sql.contains("DROP FUNCTION old_func;"));
    }
}
