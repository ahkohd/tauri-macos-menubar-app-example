use crate::schema::{
    ColumnInfo, DbSchema, EnumInfo, ForeignKeyInfo, FunctionArg, FunctionInfo, IndexInfo,
    PolicyInfo, TableInfo, TriggerInfo,
};
use sqlparser::ast::{
    AlterTable, AlterTableOperation, ColumnDef, ColumnOption, CreateFunction, CreateFunctionBody,
    CreatePolicyCommand, CreateTable, CreateTrigger, DataType, Expr,
    FunctionArg as SqlFunctionArg, FunctionArgOperator, Ident, ObjectName, OperateFunctionArg,
    Owner, Statement, TableConstraint, TriggerEvent, TriggerExecBody, TriggerObject, TriggerPeriod,
    Value,
};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::collections::HashMap;

fn normalize_table_name(name: &str) -> String {
    name.trim_start_matches("public.").to_string()
}

pub fn parse_schema_sql(sql: &str) -> Result<DbSchema, String> {
    let dialect = PostgreSqlDialect {};
    let ast = Parser::parse_sql(&dialect, sql).map_err(|e| e.to_string())?;

    let mut tables = HashMap::new();
    let mut enums = HashMap::new();
    let mut functions = HashMap::new();

    for statement in ast {
        match statement {
            Statement::CreateTable(CreateTable {
                name,
                columns,
                constraints,
                ..
            }) => {
                let raw_name = name.to_string();
                let table_name = normalize_table_name(&raw_name);
                let (parsed_columns, mut foreign_keys, indexes) =
                    parse_columns(&table_name, columns, &constraints);

                // Extract table-level constraints like Foreign Keys
                for constraint in constraints {
                    if let TableConstraint::ForeignKey(fk) = constraint {
                        if let (Some(col), Some(ref_col)) =
                            (fk.columns.first(), fk.referred_columns.first())
                        {
                            foreign_keys.push(ForeignKeyInfo {
                                constraint_name: format!("fk_{}_{}", table_name, col),
                                column_name: col.to_string(),
                                foreign_table: normalize_table_name(&fk.foreign_table.to_string()),
                                foreign_column: ref_col.to_string(),
                                on_delete: fk.on_delete
                                    .as_ref()
                                    .map(|a| a.to_string())
                                    .unwrap_or("NO ACTION".to_string()),
                            });
                        }
                    }
                }

                tables.insert(
                    table_name.clone(),
                    TableInfo {
                        table_name: table_name.clone(),
                        columns: parsed_columns,
                        foreign_keys,
                        indexes, // Indexes defined inline or via constraints
                        triggers: vec![], // Triggers are usually separate CreateTrigger statements
                        rls_enabled: false, // Need separate ALTER TABLE ... ENABLE ROW LEVEL SECURITY
                        policies: vec![], // separate CREATE POLICY
                    },
                );
            }
            Statement::CreateType {
                name,
                representation,
                ..
            } => {
                // Handle ENUMs
                if let Some(sqlparser::ast::UserDefinedTypeRepresentation::Enum { labels, .. }) = representation {
                    let enum_name = name.to_string();
                    enums.insert(
                        enum_name.clone(),
                        EnumInfo {
                            name: enum_name,
                            values: labels.iter().map(|v| v.value.clone()).collect(),
                        },
                    );
                }
            }
            Statement::CreateFunction(CreateFunction {
                name,
                args,
                return_type,
                language,
                function_body,
                ..
            }) => {
                let fn_name = name.to_string(); // parse ObjectName
                let ret_type = return_type.map(|t| t.to_string().to_lowercase()).unwrap_or("void".to_string());
                
                let mut fn_args = vec![];
                if let Some(arg_list) = args {
                    for arg in arg_list {
                        if let OperateFunctionArg { name: Some(ident), data_type, default_expr, .. } = arg {
                            let mut type_str = data_type.to_string().to_lowercase();
                            if let Some(def) = default_expr {
                                type_str.push_str(&format!(" DEFAULT {}", def));
                            }
                            fn_args.push(crate::schema::FunctionArg {
                                name: ident.value.clone(),
                                type_: type_str,
                            });
                        } else if let OperateFunctionArg { name: None, data_type, default_expr, .. } = arg {
                             let mut type_str = data_type.to_string().to_lowercase();
                             if let Some(def) = default_expr {
                                 type_str.push_str(&format!(" DEFAULT {}", def));
                             }
                             fn_args.push(crate::schema::FunctionArg {
                                name: "".to_string(),
                                type_: type_str,
                            });
                        }
                    }
                }

                let lang = language.map(|l| l.value).unwrap_or("sql".to_string());

                let def = if let Some(CreateFunctionBody::AsBeforeOptions { body, .. }) = function_body {
                    match body {
                        Expr::Value(v) => match v.value {
                             Value::DollarQuotedString(d) => d.value.trim().to_string(),
                             Value::SingleQuotedString(s) => s.trim().to_string(),
                             _ => v.to_string().trim().to_string(),
                        },
                        _ => body.to_string(),
                    }
                } else {
                    "".to_string()
                };

                functions.insert(fn_name.clone(), FunctionInfo {
                    name: fn_name,
                    args: fn_args,
                    return_type: ret_type,
                    language: lang,
                    definition: def,
                });
            }
            Statement::CreateTrigger(CreateTrigger {
                name,
                table_name,
                period,
                events,
                exec_body,
                trigger_object,
                ..
            }) => {
                let t_name = name.to_string();
                let table_target = normalize_table_name(&table_name.to_string());
                
                let ev_strs: Vec<String> = events.iter().map(|e| e.to_string()).collect();
                let timing = period.map(|p| p.to_string()).unwrap_or("BEFORE".to_string());
                
                let orientation = if let Some(obj) = trigger_object {
                     if obj.to_string().to_uppercase().contains("ROW") {
                         "ROW".to_string()
                     } else {
                         "STATEMENT".to_string()
                     }
                } else {
                    "STATEMENT".to_string()
                };

                let func_name = if let Some(TriggerExecBody { func_desc, .. }) = exec_body {
                    func_desc.name.to_string()
                } else {
                    "".to_string()
                };

                if let Some(t_info) = tables.get_mut(&table_target) {
                    t_info.triggers.push(TriggerInfo {
                        name: t_name,
                        events: ev_strs,
                        timing,
                        orientation,
                        function_name: func_name,
                    });
                }
            }
            Statement::CreatePolicy {
                name,
                table_name,
                command,
                to,
                using,
                with_check,
                ..
            } => {
                let p_name = name.value; 
                let table_target = normalize_table_name(&table_name.to_string());
                let cmd = match command.unwrap_or(CreatePolicyCommand::All) {
                    CreatePolicyCommand::All => "ALL",
                    CreatePolicyCommand::Select => "SELECT",
                    CreatePolicyCommand::Insert => "INSERT",
                    CreatePolicyCommand::Update => "UPDATE",
                    CreatePolicyCommand::Delete => "DELETE",
                    _ => "ALL",
                }.to_string();

                let roles_vec = if let Some(r_vec) = to {
                    r_vec.iter().map(|i| i.to_string()).collect()
                } else {
                    vec!["public".to_string()]
                };

                let q = using.map(|e| e.to_string());
                let wc = with_check.map(|e| e.to_string());

                if let Some(t_info) = tables.get_mut(&table_target) {
                    t_info.policies.push(PolicyInfo {
                        name: p_name,
                        cmd,
                        roles: roles_vec,
                        qual: q,
                        with_check: wc,
                    });
                }
            }
            Statement::AlterTable(AlterTable {
                 name,
                 operations,
                 ..
            }) => {
                let table_target = normalize_table_name(&name.to_string());
                if let Some(t_info) = tables.get_mut(&table_target) {
                    for op in operations {
                        match op {
                            AlterTableOperation::EnableRowLevelSecurity => t_info.rls_enabled = true,
                            AlterTableOperation::DisableRowLevelSecurity => t_info.rls_enabled = false,
                            AlterTableOperation::AddConstraint { constraint, .. } => {
                                match constraint {
                                    TableConstraint::ForeignKey(fk) => {
                                        if let (Some(col), Some(ref_col)) =
                                            (fk.columns.first(), fk.referred_columns.first())
                                        {
                                            let constraint_name = if let Some(n) = &fk.name {
                                                n.value.clone()
                                            } else {
                                                format!("fk_{}_{}", table_target, col)
                                            };

                                            t_info.foreign_keys.push(ForeignKeyInfo {
                                                constraint_name,
                                                column_name: col.to_string(),
                                                foreign_table: normalize_table_name(&fk.foreign_table.to_string()),
                                                foreign_column: ref_col.to_string(),
                                                on_delete: fk.on_delete
                                                    .as_ref()
                                                    .map(|a| a.to_string())
                                                    .unwrap_or("NO ACTION".to_string()),
                                            });
                                        }
                                    }
                                    TableConstraint::Unique(uq) => {
                                         let columns: Vec<String> = uq.columns.iter().map(|c| c.to_string()).collect();
                                         let constraint_name = if let Some(n) = &uq.name {
                                             n.value.clone()
                                         } else {
                                             format!("{}_{}_key", table_target, columns.join("_"))
                                         };
                                         
                                         t_info.indexes.push(IndexInfo {
                                             index_name: constraint_name.clone(),
                                             columns,
                                             is_unique: true,
                                             is_primary: false,
                                             owning_constraint: Some(constraint_name),
                                         });
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Statement::CreateIndex(idx) => {
                let index_name = idx.name.map(|n| n.to_string()).unwrap_or_default();
                let table_target = normalize_table_name(&idx.table_name.to_string());
                
                let index_columns: Vec<String> = idx.columns
                    .iter()
                    .map(|c| match &c.column.expr {
                        Expr::Identifier(ident) => ident.value.clone(),
                         _ => c.column.to_string(),
                    })
                    .collect();

                if let Some(t_info) = tables.get_mut(&table_target) {
                    t_info.indexes.push(IndexInfo {
                         index_name,
                         columns: index_columns,
                         is_unique: idx.unique,
                         is_primary: false,
                         owning_constraint: None,
                    });
                }
            }
            _ => {}
        }
    }

    Ok(DbSchema {
        tables,
        enums,
        functions,
    })
}

fn parse_columns(
    table_name: &str,
    columns: Vec<ColumnDef>,
    constraints: &[TableConstraint],
) -> (
    HashMap<String, ColumnInfo>,
    Vec<ForeignKeyInfo>,
    Vec<IndexInfo>,
) {
    let mut infos = HashMap::new();
    let mut fks = Vec::new();
    let mut option_indexes = Vec::new();

    for col in columns {
        let name = col.name.to_string();
        let data_type = col.data_type.to_string();
        let mut is_nullable = true;
        let mut is_primary_key = false;
        let mut is_unique = false;
        let mut column_default = None;
        let mut is_identity = false;

        for option in col.options {
            match option.option {
                ColumnOption::NotNull => is_nullable = false,
                ColumnOption::Unique { .. } => is_unique = true,
                ColumnOption::Default(expr) => column_default = Some(expr.to_string()),
                ColumnOption::Generated { .. } => is_identity = true,
                _ => {}
            }
        }

        // Check table constraints for PK/Unique
        for constraint in constraints {
            match constraint {
                TableConstraint::PrimaryKey(pk) => {
                    if pk.columns.iter().any(|c| c.to_string() == name) {
                        is_primary_key = true;
                        is_nullable = false;
                    }
                }
                TableConstraint::Unique(uq) => {
                    if uq.columns.iter().any(|c| c.to_string() == name) {
                        is_unique = true;
                    }
                }
                _ => {}
            }
        }

        infos.insert(
            name.clone(),
            ColumnInfo {
                column_name: name,
                data_type: data_type.clone(),
                is_nullable,
                column_default,
                udt_name: data_type.clone(),
                is_primary_key,
                is_unique,
                is_identity,
                enum_name: None,
                is_array: false,
            },
        );
    }

    (infos, fks, option_indexes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_full_schema() {
        let sql = r#"
CREATE OR REPLACE FUNCTION update_player_last_played() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RETURN NEW;
END;
$$;

CREATE TABLE players (
    id uuid NOT NULL
);

CREATE TRIGGER update_player_timestamp BEFORE UPDATE ON players FOR EACH ROW EXECUTE FUNCTION update_player_last_played();

CREATE POLICY "public_read" ON players FOR SELECT USING (true);

ALTER TABLE players ENABLE ROW LEVEL SECURITY;
        "#;

        let schema = parse_schema_sql(sql).expect("Failed to parse SQL");

        // Verify Function
        let func = schema.functions.get("update_player_last_played").expect("Function not found");
        assert_eq!(func.language, "plpgsql");
        assert_eq!(func.return_type, "trigger");

        // Verify Table
        let table = schema.tables.get("players").expect("Table not found");
        assert!(table.rls_enabled);

        // Verify Trigger
        assert_eq!(table.triggers.len(), 1);
        let trigger = &table.triggers[0];
        assert_eq!(trigger.name, "update_player_timestamp");
        assert_eq!(trigger.timing, "BEFORE");
        assert_eq!(trigger.orientation, "ROW");
        assert_eq!(trigger.function_name, "update_player_last_played");

        // Verify Policy
        assert_eq!(table.policies.len(), 1);
        let policy = &table.policies[0];
        assert_eq!(policy.name, "public_read");
        assert_eq!(policy.cmd, "SELECT");
    }

    #[test]
    fn test_parse_schema_mismatch() {
        let sql = r#"
CREATE TABLE players (
    id uuid NOT NULL
);

CREATE TRIGGER update_player_timestamp BEFORE UPDATE ON public.players FOR EACH ROW EXECUTE FUNCTION update_player_last_played();
        "#;

        let schema = parse_schema_sql(sql).expect("Failed to parse SQL");

        // Verify Table
        let table = schema.tables.get("players").expect("Table not found");
        
        // Verify Trigger should exist even if ON public.players
        assert_eq!(table.triggers.len(), 1);
        let trigger = &table.triggers[0];
        assert_eq!(trigger.name, "update_player_timestamp");
    }

    #[test]
    fn test_parse_function_defaults() {
        let sql = "CREATE FUNCTION generate_world(seed integer DEFAULT 0) RETURNS void LANGUAGE plpgsql AS $$BEGIN END;$$;";
        let parsed = parse_schema_sql(sql).unwrap();
        
        let func = parsed.functions.get("generate_world").expect("Function not found");
        assert_eq!(func.name, "generate_world");
        assert_eq!(func.args.len(), 1);
        assert_eq!(func.args[0].name, "seed");
        assert_eq!(func.args[0].type_, "integer DEFAULT 0");
    }

    #[test]
    fn test_function_body_trimming() {
        // SQL with extra newlines and indentation in body
        let sql = "CREATE FUNCTION test_trim() RETURNS void LANGUAGE plpgsql AS $$\n  BEGIN\n    RETURN;\n  END;\n$$;";
        let parsed = parse_schema_sql(sql).unwrap();
        let func = parsed.functions.get("test_trim").expect("Function not found");
        
        // Expect trimmed body but preserved internal whitespace
        assert_eq!(func.definition, "BEGIN\n    RETURN;\n  END;");
    }

    #[test]
    fn test_function_return_type_normalization() {
        let sql = "CREATE FUNCTION test_ret() RETURNS TRIGGER LANGUAGE plpgsql AS $$BEGIN RETURN NEW; END;$$;";
        let parsed = parse_schema_sql(sql).unwrap();
        let func = parsed.functions.get("test_ret").expect("Function not found");
        assert_eq!(func.return_type, "trigger");
        #[test]
    fn test_parse_indexes_and_constraints() {
        let sql = r#"
CREATE TABLE users ( id uuid );
CREATE UNIQUE INDEX idx_email ON users (email);
ALTER TABLE users ADD CONSTRAINT fk_role FOREIGN KEY (role_id) REFERENCES roles(id);
ALTER TABLE users ADD CONSTRAINT unique_username UNIQUE (username);
"#;
        let schema = parse_schema_sql(sql).expect("Failed to parse SQL");
        let table = schema.tables.get("users").expect("Table not found");

        // Verify CREATE INDEX
        assert!(table.indexes.iter().any(|i| i.index_name == "idx_email" && i.is_unique));

        // Verify ALTER TABLE FK
        assert!(table.foreign_keys.iter().any(|fk| fk.constraint_name == "fk_role"));

        // Verify ALTER TABLE UNIQUE (should be an index with constraint)
        assert!(table.indexes.iter().any(|i| i.index_name == "unique_username" && i.owning_constraint.as_deref() == Some("unique_username")));
    }
}
}
