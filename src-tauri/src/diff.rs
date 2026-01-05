use crate::schema::{
    DbSchema, EnumInfo, ForeignKeyInfo, FunctionInfo, IndexInfo, PolicyInfo, TableInfo, TriggerInfo,
};
use std::collections::HashMap;

#[derive(Debug)]
pub struct SchemaDiff {
    pub tables_to_create: Vec<String>,
    pub tables_to_drop: Vec<String>,
    pub table_changes: HashMap<String, TableDiff>,
    pub enum_changes: Vec<EnumChange>,
    pub functions_to_create: Vec<FunctionInfo>,
    pub functions_to_drop: Vec<String>,
    pub functions_to_update: Vec<FunctionInfo>, // To handle REPLACE
}

#[derive(Debug)]
pub struct TableDiff {
    pub columns_to_add: Vec<String>,
    pub columns_to_drop: Vec<String>,
    pub columns_to_modify: Vec<ColumnModification>,
    pub rls_change: Option<bool>, // None = no change, Some(true) = enable, Some(false) = disable
    pub policies_to_create: Vec<PolicyInfo>,
    pub policies_to_drop: Vec<PolicyInfo>,
    pub triggers_to_create: Vec<TriggerInfo>,
    pub triggers_to_drop: Vec<TriggerInfo>,
    pub indexes_to_create: Vec<IndexInfo>,
    pub indexes_to_drop: Vec<IndexInfo>,
}

#[derive(Debug)]
pub struct ColumnModification {
    pub column_name: String,
    pub changes: ColumnChangeDetail,
}

#[derive(Debug)]
pub struct ColumnChangeDetail {
    pub type_change: Option<(String, String)>, // (from, to)
    pub nullable_change: Option<(bool, bool)>, // (from, to)
}

#[derive(Debug)]
pub struct EnumChange {
    pub type_: EnumChangeType,
    pub name: String,
    pub values: Option<Vec<String>>,
}

#[derive(Debug, PartialEq)]
pub enum EnumChangeType {
    Create,
    Drop,
    AddValue,
}

pub fn compute_diff(remote: &DbSchema, local: &DbSchema) -> SchemaDiff {
    let mut diff = SchemaDiff {
        tables_to_create: vec![],
        tables_to_drop: vec![],
        table_changes: HashMap::new(),
        enum_changes: vec![],
        functions_to_create: vec![],
        functions_to_drop: vec![],
        functions_to_update: vec![],
    };

    // 1. Tables
    for (name, _) in &local.tables {
        if !remote.tables.contains_key(name) {
            diff.tables_to_create.push(name.clone());
        }
    }

    for (name, _) in &remote.tables {
        if !local.tables.contains_key(name) {
            diff.tables_to_drop.push(name.clone());
        }
    }

    // Table modifications
    for (name, local_table) in &local.tables {
        if let Some(remote_table) = remote.tables.get(name) {
            let table_diff = compute_table_diff(remote_table, local_table);
            if !table_diff.is_empty() {
                diff.table_changes.insert(name.clone(), table_diff);
            }
        }
    }

    // 2. Enums
    // simplified enum diff : create/drop
    // TODO: support AddValue properly by checking values mismatch
    for (name, info) in &local.enums {
        if !remote.enums.contains_key(name) {
            diff.enum_changes.push(EnumChange {
                type_: EnumChangeType::Create,
                name: name.clone(),
                values: Some(info.values.clone()),
            });
        } 
        // else check for added values? Skipping for now as per minimal scope, 
        // but nice to have. logic existed in original code? 
        // Original code: "for (name, info) in &local.enums { if !remote... }"
        // It didn't handle updates. I will leave as is for now.
    }

    // 3. Functions
    // Function identity is complex (name + args). 
    // Supawatch currently keys functions by 'name' only in DbSchema (prob incorrect for overloads, but we stick to it).
    for (name, local_func) in &local.functions {
        if let Some(remote_func) = remote.functions.get(name) {
            // Check if changed
            if local_func != remote_func {
                // REPLACE
                diff.functions_to_update.push(local_func.clone());
            }
        } else {
            // Create
            diff.functions_to_create.push(local_func.clone());
        }
    }

    for (name, _) in &remote.functions {
        if !local.functions.contains_key(name) {
            diff.functions_to_drop.push(name.clone());
        }
    }

    diff
}

fn compute_table_diff(remote: &TableInfo, local: &TableInfo) -> TableDiff {
    let mut diff = TableDiff {
        columns_to_add: vec![],
        columns_to_drop: vec![],
        columns_to_modify: vec![],
        rls_change: None,
        policies_to_create: vec![],
        policies_to_drop: vec![],
        triggers_to_create: vec![],
        triggers_to_drop: vec![],
        indexes_to_create: vec![],
        indexes_to_drop: vec![],
    };

    // Columns
    for (name, _) in &local.columns {
        if !remote.columns.contains_key(name) {
            diff.columns_to_add.push(name.clone());
        }
    }

    for (name, _) in &remote.columns {
        if !local.columns.contains_key(name) {
            diff.columns_to_drop.push(name.clone());
        }
    }

    // Modifications
    for (name, local_col) in &local.columns {
        if let Some(remote_col) = remote.columns.get(name) {
            let mut changes = ColumnChangeDetail {
                type_change: None,
                nullable_change: None,
            };

            // Loose comparison for types (e.g. "text" vs "TEXT")
            if local_col.data_type.to_lowercase() != remote_col.data_type.to_lowercase() {
                changes.type_change = Some((
                    remote_col.data_type.clone(),
                    local_col.data_type.clone(),
                ));
            }

            if local_col.is_nullable != remote_col.is_nullable {
                changes.nullable_change = Some((remote_col.is_nullable, local_col.is_nullable));
            }

            if changes.type_change.is_some() || changes.nullable_change.is_some() {
                diff.columns_to_modify.push(ColumnModification {
                    column_name: name.clone(),
                    changes,
                });
            }
        }
    }

    // RLS Status
    if local.rls_enabled != remote.rls_enabled {
        diff.rls_change = Some(local.rls_enabled);
    }

    // Policies
    // Identify by name (policy names must be unique per table)
    let remote_policies: HashMap<&String, &PolicyInfo> = remote.policies.iter().map(|p| (&p.name, p)).collect();
    let local_policies: HashMap<&String, &PolicyInfo> = local.policies.iter().map(|p| (&p.name, p)).collect();

    for p in &local.policies {
        if !remote_policies.contains_key(&p.name) {
            diff.policies_to_create.push(p.clone());
        } else {
            // Check for modification?
            // "CREATE OR REPLACE POLICY" isn't standard, it's DROP + CREATE or ALTER.
            // But usually we can just DROP AND CREATE if it changed.
            // For now, simpler: if not exact match, Drop + Create
            let remote_p = remote_policies.get(&p.name).unwrap();
            if p != *remote_p {
                 diff.policies_to_drop.push((*remote_p).clone());
                 diff.policies_to_create.push(p.clone());
            }
        }
    }

    for p in &remote.policies {
        if !local_policies.contains_key(&p.name) {
            diff.policies_to_drop.push(p.clone());
        }
    }

    // Triggers
    // Identify by name
    let remote_triggers: HashMap<&String, &TriggerInfo> = remote.triggers.iter().map(|t| (&t.name, t)).collect();
    let local_triggers: HashMap<&String, &TriggerInfo> = local.triggers.iter().map(|t| (&t.name, t)).collect();

    for t in &local.triggers {
        if !remote_triggers.contains_key(&t.name) {
            diff.triggers_to_create.push(t.clone());
        } else {
            let remote_t = remote_triggers.get(&t.name).unwrap();
            if t != *remote_t {
                // DROP + CREATE
                diff.triggers_to_drop.push((*remote_t).clone());
                diff.triggers_to_create.push(t.clone());
            }
        }
    }

    for t in &remote.triggers {
        if !local_triggers.contains_key(&t.name) {
            diff.triggers_to_drop.push(t.clone());
        }
    }

    // Indexes
    // Identify by name
    let remote_indexes: HashMap<&String, &IndexInfo> = remote.indexes.iter().map(|i| (&i.index_name, i)).collect();
    let local_indexes: HashMap<&String, &IndexInfo> = local.indexes.iter().map(|i| (&i.index_name, i)).collect();

    for i in &local.indexes {
        if !remote_indexes.contains_key(&i.index_name) {
            diff.indexes_to_create.push(i.clone());
        } else {
             let remote_i = remote_indexes.get(&i.index_name).unwrap();
             // Compare content: columns, unique, primary
             // Note: columns order matters
             if i.columns != remote_i.columns || i.is_unique != remote_i.is_unique || i.is_primary != remote_i.is_primary {
                 // DROP + CREATE
                 diff.indexes_to_drop.push((*remote_i).clone());
                 diff.indexes_to_create.push(i.clone());
             }
        }
    }

    for i in &remote.indexes {
        if !local_indexes.contains_key(&i.index_name) {
             diff.indexes_to_drop.push(i.clone());
        }
    }

    diff
}

impl TableDiff {
    pub fn is_empty(&self) -> bool {
        self.columns_to_add.is_empty()
            && self.columns_to_drop.is_empty()
            && self.columns_to_modify.is_empty()
            && self.rls_change.is_none()
            && self.policies_to_create.is_empty()
            && self.policies_to_drop.is_empty()
            && self.triggers_to_create.is_empty()
            && self.triggers_to_drop.is_empty()
            && self.indexes_to_create.is_empty()
            && self.indexes_to_drop.is_empty()
    }
}

impl SchemaDiff {
    pub fn summarize(&self) -> String {
        let mut parts = vec![];

        for table in &self.tables_to_create {
            parts.push(format!("+ Table '{}'", table));
        }

        for table in &self.tables_to_drop {
            parts.push(format!("- Table '{}'", table));
        }

        for (table_name, diff) in &self.table_changes {
            for col in &diff.columns_to_add {
                parts.push(format!("+ Column '{}.{}'", table_name, col));
            }
            for col in &diff.columns_to_drop {
                parts.push(format!("- Column '{}.{}'", table_name, col));
            }
            for mod_col in &diff.columns_to_modify {
                let mut changes = vec![];
                if let Some((from, to)) = &mod_col.changes.type_change {
                    changes.push(format!("type: {} -> {}", from, to));
                }
                if let Some((from, to)) = mod_col.changes.nullable_change {
                    let from_str = if from { "NULL" } else { "NOT NULL" };
                    let to_str = if to { "NULL" } else { "NOT NULL" };
                    changes.push(format!("nullable: {} -> {}", from_str, to_str));
                }
                parts.push(format!(
                    "~ Column '{}.{}' ({})",
                    table_name,
                    mod_col.column_name,
                    changes.join(", ")
                ));
            }

            if let Some(rls) = diff.rls_change {
                parts.push(format!("~ Table '{}' RLS: {}", table_name, if rls { "ENABLE" } else { "DISABLE" }));
            }

            for p in &diff.policies_to_create {
                parts.push(format!("+ Policy '{}' ON '{}'", p.name, table_name));
            }
            for p in &diff.policies_to_drop {
                parts.push(format!("- Policy '{}' ON '{}'", p.name, table_name));
            }

            for t in &diff.triggers_to_create {
                parts.push(format!("+ Trigger '{}' ON '{}'", t.name, table_name));
            }
            for t in &diff.triggers_to_drop {
                parts.push(format!("- Trigger '{}' ON '{}'", t.name, table_name));
            }
            
            for i in &diff.indexes_to_create {
                 parts.push(format!("+ Index '{}' ON '{}'", i.index_name, table_name));
            }
             for i in &diff.indexes_to_drop {
                 parts.push(format!("- Index '{}' ON '{}'", i.index_name, table_name));
            }
        }

        for enum_change in &self.enum_changes {
            match enum_change.type_ {
                EnumChangeType::Create => {
                    parts.push(format!("+ Enum '{}'", enum_change.name));
                }
                EnumChangeType::Drop => {
                    parts.push(format!("- Enum '{}'", enum_change.name));
                }
                EnumChangeType::AddValue => {
                    parts.push(format!("~ Enum '{}' (add values)", enum_change.name));
                }
            }
        }

        for f in &self.functions_to_create {
            parts.push(format!("+ Function '{}'", f.name));
        }
        for f in &self.functions_to_drop {
            parts.push(format!("- Function '{}'", f));
        }
        for f in &self.functions_to_update {
            parts.push(format!("~ Function '{}'", f.name));
        }

        if parts.is_empty() {
            return "No changes detected".to_string();
        }

        parts.sort();
        parts.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summarize() {
        let mut diff = SchemaDiff {
            tables_to_create: vec!["users".to_string()],
            tables_to_drop: vec!["posts".to_string()],
            table_changes: HashMap::new(),
            enum_changes: vec![],
            functions_to_create: vec![],
            functions_to_drop: vec![],
            functions_to_update: vec![],
        };
        
        // Add some column modifications
        let mut table_diff = TableDiff {
            columns_to_add: vec!["created_at".to_string()],
            columns_to_drop: vec![],
            columns_to_modify: vec![ColumnModification {
                column_name: "name".to_string(),
                changes: ColumnChangeDetail {
                    type_change: Some(("text".to_string(), "varchar".to_string())),
                    nullable_change: None,
                },
            }],
            rls_change: None,
            policies_to_create: vec![],
            policies_to_drop: vec![],
            triggers_to_create: vec![],
            triggers_to_drop: vec![],
            indexes_to_create: vec![],
            indexes_to_drop: vec![],
        };
        diff.table_changes.insert("profiles".to_string(), table_diff);

        let summary = diff.summarize();
        assert!(summary.contains("+ Table 'users'"));
        assert!(summary.contains("- Table 'posts'"));
        assert!(summary.contains("+ Column 'profiles.created_at'"));
        assert!(summary.contains("~ Column 'profiles.name' (type: text -> varchar)"));
    }

    #[test]
    fn test_compute_diff_full() {
        use crate::schema::*;

        // Setup "Local" schema with all features
        let mut local = DbSchema::new();
        
        let mut table = TableInfo {
            table_name: "users".to_string(),
            columns: HashMap::new(),
            foreign_keys: vec![],
            indexes: vec![],
            triggers: vec![],
            rls_enabled: true,
            policies: vec![],
        };
        table.policies.push(PolicyInfo {
            name: "policy1".to_string(),
            cmd: "SELECT".to_string(),
            roles: vec!["public".to_string()],
            qual: Some("true".to_string()),
            with_check: None,
        });
        table.triggers.push(TriggerInfo {
            name: "trigger1".to_string(),
            events: vec!["INSERT".to_string()],
            timing: "BEFORE".to_string(),
            orientation: "ROW".to_string(),
            function_name: "my_func".to_string(),
        });
        local.tables.insert("users".to_string(), table);

        local.functions.insert("my_func".to_string(), FunctionInfo {
            name: "my_func".to_string(),
            args: vec![],
            return_type: "trigger".to_string(),
            language: "plpgsql".to_string(),
            definition: "BEGIN RETURN NEW; END;".to_string(),
        });

        // Remote is empty
        let remote = DbSchema::new();

        // 1. Local -> Remote (Push logic: Local has stuff, Remote empty -> Create stuff)
        // Wait, compute_diff(remote, local) -> Diff to make Remote like Local
        // So SchemaDiff should contain CREATES.
        let diff = compute_diff(&remote, &local); // remote is target? No. 
        // diff definition: "compute_diff(remote, local)".
        // Tables: if !remote.contains(key) -> Create. 
        // So it creates things present in LOCAL but missing in REMOTE.
        // Yes.
        
        assert_eq!(diff.tables_to_create, vec!["users"]);
        assert_eq!(diff.functions_to_create.len(), 1);
        assert_eq!(diff.functions_to_create[0].name, "my_func");
        
        // Table details are inside tables_to_create handling in generator, 
        // but for existing tables, we check table_changes.
        // Since "users" is in tables_to_create, it won't be in table_changes.
        // Let's make "users" exist in remote but be empty.
        
        let mut remote2 = DbSchema::new();
        remote2.tables.insert("users".to_string(), TableInfo {
             table_name: "users".to_string(),
             columns: HashMap::new(),
             foreign_keys: vec![],
             indexes: vec![],
             triggers: vec![],
             rls_enabled: false,
             policies: vec![],
        });

        let diff2 = compute_diff(&remote2, &local);
        assert!(diff2.tables_to_create.is_empty());
        
        let table_diff = diff2.table_changes.get("users").unwrap();
        assert_eq!(table_diff.rls_change, Some(true));
        assert_eq!(table_diff.policies_to_create.len(), 1);
        assert_eq!(table_diff.policies_to_create[0].name, "policy1");
        assert_eq!(table_diff.triggers_to_create.len(), 1);
        assert_eq!(table_diff.triggers_to_create[0].name, "trigger1");
    }
}
