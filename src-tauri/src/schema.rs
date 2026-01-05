use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbSchema {
    pub tables: HashMap<String, TableInfo>,
    pub enums: HashMap<String, EnumInfo>,
    pub functions: HashMap<String, FunctionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TableInfo {
    pub table_name: String,
    pub columns: HashMap<String, ColumnInfo>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
    pub indexes: Vec<IndexInfo>,
    pub triggers: Vec<TriggerInfo>,
    pub rls_enabled: bool,
    pub policies: Vec<PolicyInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ColumnInfo {
    pub column_name: String,
    pub data_type: String,
    pub is_nullable: bool,
    pub column_default: Option<String>,
    pub udt_name: String,
    pub is_primary_key: bool,
    pub is_unique: bool,
    pub is_identity: bool,
    pub enum_name: Option<String>, // Helper for mapping back to enums
    pub is_array: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForeignKeyInfo {
    pub constraint_name: String,
    pub column_name: String,
    pub foreign_table: String,
    pub foreign_column: String,
    pub on_delete: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexInfo {
    pub index_name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
    pub owning_constraint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnumInfo {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PolicyInfo {
    pub name: String,
    pub cmd: String, // "SELECT", "INSERT", "UPDATE", "DELETE", "ALL"
    pub roles: Vec<String>,
    pub qual: Option<String>,       // using
    pub with_check: Option<String>, // with check
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerInfo {
    pub name: String,
    pub events: Vec<String>, // INSERT, UPDATE, etc.
    pub timing: String,      // BEFORE, AFTER, INSTEAD OF
    pub orientation: String, // ROW, STATEMENT
    pub function_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionInfo {
    pub name: String,
    pub args: Vec<FunctionArg>,
    pub return_type: String,
    pub language: String,
    pub definition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionArg {
    pub name: String,
    pub type_: String,
}

impl DbSchema {
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
            enums: HashMap::new(),
            functions: HashMap::new(),
        }
    }
}
