import type { DbSchema, TableInfo } from "./introspect";

export function generateTypeScript(dbSchema: DbSchema): string {
  const imports = new Set<string>(["table"]);
  const lines: string[] = [];
  const enumNames = new Map<string, string>(); // dbName -> exportName
  const tableNames = new Map<string, string>(); // dbName -> exportName
  const functionNames = new Map<string, string>(); // dbName -> exportName

  // 1. Generate Enums
  if (dbSchema.enums.size > 0) {
    imports.add("enumType");
    for (const [name, enumInfo] of dbSchema.enums) {
      const exportName = toCamelCase(name) + "Enum";
      enumNames.set(name, exportName);

      const values = enumInfo.values.map((v) => `  "${v}",`).join("\n");
      lines.push(
        `export const ${exportName} = enumType("${name}", [\n${values}\n] as const);\n`
      );
    }
  }

  // 1.5 Generate Functions
  if (dbSchema.functions && dbSchema.functions.size > 0) {
    imports.add("pgFunction");
    for (const [name, funcInfo] of dbSchema.functions) {
      const exportName = toCamelCase(name) + "Func";
      functionNames.set(name, exportName);

      const args = funcInfo.args
        .map(
          (a) =>
            `    { name: "${a.name}", type: "${a.type.replace(/"/g, '\\"')}" },`
        )
        .join("\n");

      // Definition needs proper escaping if it contains quotes
      const definition = funcInfo.definition.replace(/`/g, "\\`").trim();

      lines.push(`export const ${exportName} = pgFunction("${name}", {`);
      if (funcInfo.args.length > 0) {
        lines.push(`  args: [\n${args}\n  ],`);
      }
      lines.push(`  returns: "${funcInfo.return_type}",`);
      lines.push(`  language: "${funcInfo.language}",`);
      lines.push("  definition: `");
      lines.push(definition);
      lines.push("`,");
      lines.push("});\n");
    }
  }

  // 2. Generate Tables
  const allTables = Array.from(dbSchema.tables.values());
  const sortedTables = sortTablesByDependency(allTables);

  for (const table of sortedTables) {
    const exportName = toCamelCase(table.table_name);
    tableNames.set(table.table_name, exportName);
  }

  for (const table of sortedTables) {
    const exportName = tableNames.get(table.table_name)!;
    lines.push(`export const ${exportName} = table("${table.table_name}", {`);

    for (const [colName, col] of table.columns) {
      const fieldName = colName;

      let pgType = col.data_type;
      let isArray = false;

      // Handle array types
      if (pgType === "ARRAY" && col.udt_name.startsWith("_")) {
        isArray = true;
        pgType = col.udt_name.substring(1);

        // Map internal type names to standard names
        if (pgType === "int4") pgType = "integer";
        if (pgType === "int8") pgType = "bigint";
        if (pgType === "int2") pgType = "smallint";
        if (pgType === "bool") pgType = "boolean";
        if (pgType === "float4") pgType = "real";
        if (pgType === "float8") pgType = "double precision";
      }

      let typeFn = mapPostgresToSchemaType(pgType);

      // Handle Enums
      if (col.udt_name && dbSchema.enums.has(col.udt_name)) {
        const enumExport = enumNames.get(col.udt_name);
        if (enumExport) {
          typeFn = enumExport;
        }
      } else if (isArray && dbSchema.enums.has(pgType)) {
        // Handle array of enums
        const enumExport = enumNames.get(pgType);
        if (enumExport) {
          typeFn = enumExport;
        }
      } else {
        // Add type to imports if it's a standard type
        if (typeFn !== "user-defined" && !dbSchema.enums.has(typeFn)) {
          imports.add(typeFn);
        }
      }

      const options: string[] = [];

      if (col.is_primary_key) options.push("primaryKey: true");
      if (!col.is_nullable && !col.is_primary_key)
        options.push("notNull: true");

      // Unique logic: prioritize explicit unique constraint option
      if (col.is_unique) {
        options.push("unique: true");
      }

      if (col.column_default !== null) {
        // Special case: UUID primary keys with gen_random_uuid() default
        if (
          typeFn === "uuid" &&
          col.is_primary_key &&
          (col.column_default === "gen_random_uuid()" ||
            col.column_default.includes("uuid_generate_v4()"))
        ) {
          // Skip default value
        } else {
          // Clean up default value
          let defVal = col.column_default;
          // Postgres defaults often look like 'value'::type or nextval(...)
          if (defVal.includes("::")) {
            defVal = defVal.split("::")[0].replace(/^'|'$/g, "");
          }

          // Handle numeric/boolean defaults
          if (
            typeFn === "integer" ||
            typeFn === "smallint" ||
            typeFn === "bigint" ||
            typeFn === "real" ||
            typeFn === "doublePrecision" ||
            typeFn === "numeric" ||
            typeFn === "decimal"
          ) {
            if (!isNaN(Number(defVal))) {
              // it's a number
            } else {
              // might be a string expression
              defVal = `"${defVal}"`;
            }
          } else if (typeFn === "boolean") {
            if (defVal === "true" || defVal === "false") {
              // bool literal
            } else {
              defVal = `"${defVal}"`;
            }
          } else if (defVal.startsWith("'") && defVal.endsWith("'")) {
            // already quoted
            defVal = `"${defVal.slice(1, -1)}"`;
          } else if (!defVal.startsWith('"')) {
            // wrap in quotes if not number/bool/function
            if (!defVal.endsWith(")")) {
              defVal = `"${defVal}"`;
            } else {
              // It's a function call, pass as string
              defVal = `"${defVal}"`;
            }
          }

          options.push(`defaultValue: ${defVal}`);
        }
      }

      let colDef = `  ${fieldName}: ${typeFn}(${
        options.length > 0 ? `{ ${options.join(", ")} }` : ""
      })`;

      // Chain methods

      if (isArray) {
        colDef += `.array()`;
      }

      // References
      const fk = table.foreignKeys.find((f) => f.column_name === colName);
      if (fk) {
        const foreignExport = tableNames.get(fk.foreign_table);
        if (foreignExport) {
          colDef += `\n    .references(() => ${foreignExport}.${fk.foreign_column})`;
        }
      }

      // Indexes
      const idx = table.indexes.find(
        (i) => i.columns.length === 1 && i.columns[0] === colName
      );
      if (idx) {
        if (idx.is_unique) {
          // If we already marked the column as unique (constraint), we might skip .uniqueIndex()
          // if it's the backing index.
          // But identifying if it's the same index is hard.
          // However, if we have `unique: true`, we get a unique constraint + implicit index.
          // If we also add `.uniqueIndex()`, we get a second index.
          // To be safe and avoid "unique: true -> false" diffs, we generated `unique: true`.
          // Now we check if we should skip .uniqueIndex().
          // If col.is_unique is true, it implies there is a unique index.
          // So if col.is_unique is true, we assume this index matches the constraint and we SKIP explicit uniqueIndex.
          // Unless there are multiple unique indexes? But that's rare for one column.

          if (!col.is_unique) {
            colDef += `.uniqueIndex()`;
          }
          // Note: If col.is_unique is true (constraint exists), we rely on that to create the implicit index.
          // We DO NOT add .uniqueIndex() to avoid duplicates or diff confusion.
          // This matches how we handle it in push/diff logic.
        } else {
          // It's a non-unique index.
          // Check if this index is implicitly created by a foreign key?
          // Postgres doesn't automatically index FKs, so we generally want to keep this.
          // UNLESS the user uses .index() in their schema definition which we want to reproduce.
          // However, we can't easily distinguish manual vs implicit indexes (except PK/Unique constraints).
          // So we default to adding .index() for non-unique single-column indexes.
          colDef += `.index()`;
        }
      }

      colDef += ",";
      lines.push(colDef);
    }

    lines.push("})");

    // Add RLS and Policies
    if (table.rls_enabled) {
      lines.push("  .enableRLS()");
    }

    if (table.policies && table.policies.length > 0) {
      for (const policy of table.policies) {
        let policyDef = `  .policy("${policy.name}", { for: "${policy.cmd.toLowerCase()}"`;

        if (
          policy.roles &&
          policy.roles.length > 0 &&
          !policy.roles.includes("public")
        ) {
          const roles = policy.roles.map((r) => `"${r}"`).join(", ");
          policyDef += `, to: [${roles}]`;
        }

        if (policy.qual) {
          policyDef += `, using: "${policy.qual.replace(/"/g, '\\"')}"`;
        }

        if (policy.with_check) {
          policyDef += `, withCheck: "${policy.with_check.replace(/"/g, '\\"')}"`;
        }

        policyDef += " })";
        lines.push(policyDef);
      }
    }

    // Add Triggers
    if (table.triggers && table.triggers.length > 0) {
      for (const trigger of table.triggers) {
        let triggerDef = `  .trigger("${trigger.name}", { events: [${trigger.events
          .map((e) => `"${e}"`)
          .join(", ")}], when: "${trigger.timing}"`;

        if (trigger.orientation && trigger.orientation !== "ROW") {
          triggerDef += `, forEach: "${trigger.orientation}"`;
        }

        const funcExport = functionNames.get(trigger.function_name);
        if (funcExport) {
          triggerDef += `, call: ${funcExport}`;
        } else {
          // If function is not in schema map (maybe system function or not introspected), use string
          triggerDef += `, call: "${trigger.function_name}"`;
        }

        triggerDef += " })";
        lines.push(triggerDef);
      }
    }

    // Add Composite Indexes
    if (table.indexes) {
      for (const idx of table.indexes) {
        if (idx.columns.length > 1) {
          const cols = idx.columns.map((c) => `"${c}"`).join(", ");
          if (idx.is_unique) {
            lines.push(`  .unique([${cols}])`);
          } else {
            lines.push(`  .index([${cols}])`);
          }
        }
      }
    }

    lines.push(";\n");
  }

  // 3. Generate Schema Export
  lines.push("// Export the complete schema");
  lines.push("export const schema = {");
  for (const name of tableNames.values()) {
    lines.push(`  ${name},`);
  }
  for (const name of functionNames.values()) {
    lines.push(`  ${name},`);
  }
  lines.push("} as const;\n");

  lines.push("// Export the schema type for use in hooks");
  lines.push("export type AppSchema = typeof schema;");

  // Construct final file
  const importsArr = Array.from(imports).sort();
  const header = `import {\n  ${importsArr.join(
    ",\n  "
  )},\n} from "supabase-schema";\n`;

  return header + "\n" + lines.join("\n");
}

function mapPostgresToSchemaType(pgType: string): string {
  const normalized = pgType.toLowerCase();

  if (normalized === "character varying") return "varchar";
  if (normalized === "character") return "char";
  if (normalized === "timestamp without time zone") return "timestamp";
  if (normalized === "timestamp with time zone") return "timestamptz";
  if (normalized === "time without time zone") return "time";
  if (normalized === "double precision") return "doublePrecision";

  // Direct matches
  const direct = [
    "text",
    "integer",
    "smallint",
    "bigint",
    "serial",
    "bigserial",
    "boolean",
    "date",
    "uuid",
    "json",
    "jsonb",
    "real",
    "numeric",
    "decimal",
  ];

  if (direct.includes(normalized)) return normalized;

  return "text"; // Fallback
}

function toCamelCase(str: string): string {
  return str.replace(/_([a-z])/g, (g) => g[1].toUpperCase());
}

function hasUniqueIndex(table: TableInfo, colName: string): boolean {
  return table.indexes.some(
    (i) => i.is_unique && i.columns.length === 1 && i.columns[0] === colName
  );
}

function sortTablesByDependency(tables: TableInfo[]): TableInfo[] {
  const tableMap = new Map(tables.map((t) => [t.table_name, t]));
  const visited = new Set<string>();
  const result: TableInfo[] = [];
  const visiting = new Set<string>(); // Detect cycles

  function visit(tableName: string) {
    if (visited.has(tableName)) return;
    if (visiting.has(tableName)) {
      return;
    }

    visiting.add(tableName);

    const table = tableMap.get(tableName);
    if (!table) return;

    // Visit dependencies first
    for (const fk of table.foreignKeys) {
      if (fk.foreign_table !== tableName && tableMap.has(fk.foreign_table)) {
        visit(fk.foreign_table);
      }
    }

    visiting.delete(tableName);
    visited.add(tableName);
    result.push(table);
  }

  for (const table of tables) {
    visit(table.table_name);
  }

  return result;
}
