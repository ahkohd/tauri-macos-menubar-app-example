import type pg from "pg";

export interface ColumnInfo {
  column_name: string;
  data_type: string;
  is_nullable: boolean;
  column_default: string | null;
  udt_name: string;
  is_primary_key: boolean;
  is_unique: boolean;
  is_identity: boolean;
}

export interface ForeignKeyInfo {
  constraint_name: string;
  column_name: string;
  foreign_table: string;
  foreign_column: string;
  on_delete: string;
}

export interface IndexInfo {
  index_name: string;
  columns: string[];
  is_unique: boolean;
  is_primary: boolean;
}

export interface EnumInfo {
  name: string;
  values: string[];
}

export interface PolicyInfo {
  name: string;
  cmd: string; // "SELECT", "INSERT", "UPDATE", "DELETE", "ALL"
  roles: string[];
  qual: string | null; // using
  with_check: string | null; // with check
}

export interface TriggerInfo {
  name: string;
  events: string[]; // INSERT, UPDATE, etc.
  timing: string; // BEFORE, AFTER, INSTEAD OF
  orientation: string; // ROW, STATEMENT
  function_name: string;
}

export interface FunctionInfo {
  name: string;
  args: { name: string; type: string }[];
  return_type: string;
  language: string;
  definition: string;
}

export interface TableInfo {
  table_name: string;
  columns: Map<string, ColumnInfo>;
  foreignKeys: ForeignKeyInfo[];
  indexes: IndexInfo[];
  triggers: TriggerInfo[];
  rls_enabled: boolean;
  policies: PolicyInfo[];
}

export interface DbSchema {
  tables: Map<string, TableInfo>;
  enums: Map<string, EnumInfo>;
  functions: Map<string, FunctionInfo>;
}

export async function introspectDatabase(client: pg.Client): Promise<DbSchema> {
  const schema: DbSchema = {
    tables: new Map(),
    enums: new Map(),
    functions: new Map(),
  };

  // Get all enums
  const enumsResult = await client.query(`
    SELECT t.typname as name, array_agg(e.enumlabel ORDER BY e.enumsortorder) as values
    FROM pg_type t
    JOIN pg_enum e ON t.oid = e.enumtypid
    JOIN pg_namespace n ON t.typnamespace = n.oid
    WHERE n.nspname = 'public'
    GROUP BY t.typname
  `);

  for (const row of enumsResult.rows) {
    schema.enums.set(row.name, {
      name: row.name,
      // pg driver may return array_agg as string "{val1,val2}" or actual array
      values: Array.isArray(row.values)
        ? row.values
        : parsePostgresArray(row.values),
    });
  }

  // Get all functions
  const functionsResult = await client.query(`
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
  `);

  for (const row of functionsResult.rows) {
    schema.functions.set(row.name, {
      name: row.name,
      args: parseFunctionArgs(row.args),
      return_type: row.return_type,
      language: row.language,
      definition: row.definition,
    });
  }

  // Get all tables
  const tablesResult = await client.query(`
    SELECT table_name
    FROM information_schema.tables
    WHERE table_schema = 'public'
    AND table_type = 'BASE TABLE'
  `);

  for (const row of tablesResult.rows) {
    const tableName = row.table_name;

    // Get columns with constraints
    const columnsResult = await client.query(
      `
      SELECT
        c.column_name,
        c.data_type,
        c.is_nullable,
        c.column_default,
        c.udt_name,
        c.is_identity,
        COALESCE(pk.is_primary, false) as is_primary_key,
        COALESCE(uq.is_unique, false) as is_unique
      FROM information_schema.columns c
      LEFT JOIN (
        SELECT kcu.column_name, true as is_primary
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
          AND tc.table_schema = kcu.table_schema
        WHERE tc.constraint_type = 'PRIMARY KEY'
        AND tc.table_schema = 'public'
        AND tc.table_name = $1
      ) pk ON c.column_name = pk.column_name
      LEFT JOIN (
        SELECT kcu.column_name, true as is_unique
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
          AND tc.table_schema = kcu.table_schema
        WHERE tc.constraint_type = 'UNIQUE'
        AND tc.table_schema = 'public'
        AND tc.table_name = $1
      ) uq ON c.column_name = uq.column_name
      WHERE c.table_schema = 'public' AND c.table_name = $1
    `,
      [tableName]
    );

    const columns = new Map<string, ColumnInfo>();
    for (const col of columnsResult.rows) {
      columns.set(col.column_name, {
        column_name: col.column_name,
        data_type: col.data_type,
        is_nullable: col.is_nullable === "YES",
        column_default: col.column_default,
        udt_name: col.udt_name,
        is_primary_key: col.is_primary_key,
        is_unique: col.is_unique,
        is_identity: col.is_identity === "YES",
      });
    }

    // Get foreign keys
    const fkResult = await client.query(
      `
      SELECT
        tc.constraint_name,
        kcu.column_name,
        ccu.table_name AS foreign_table,
        ccu.column_name AS foreign_column,
        rc.delete_rule as on_delete
      FROM information_schema.table_constraints tc
      JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
        AND tc.table_schema = kcu.table_schema
      JOIN information_schema.constraint_column_usage ccu
        ON ccu.constraint_name = tc.constraint_name
        AND ccu.table_schema = tc.table_schema
      JOIN information_schema.referential_constraints rc
        ON rc.constraint_name = tc.constraint_name
        AND rc.constraint_schema = tc.table_schema
      WHERE tc.constraint_type = 'FOREIGN KEY'
      AND tc.table_schema = 'public'
      AND tc.table_name = $1
    `,
      [tableName]
    );

    const foreignKeys: ForeignKeyInfo[] = fkResult.rows.map((row) => ({
      constraint_name: row.constraint_name,
      column_name: row.column_name,
      foreign_table: row.foreign_table,
      foreign_column: row.foreign_column,
      on_delete: row.on_delete,
    }));

    // Get indexes (including multi-column)
    const indexResult = await client.query(
      `
      SELECT
        i.relname as index_name,
        array_agg(a.attname ORDER BY array_position(ix.indkey, a.attnum)) as columns,
        ix.indisunique as is_unique,
        ix.indisprimary as is_primary
      FROM pg_class t
      JOIN pg_index ix ON t.oid = ix.indrelid
      JOIN pg_class i ON i.oid = ix.indexrelid
      JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey)
      JOIN pg_namespace n ON t.relnamespace = n.oid
      WHERE n.nspname = 'public'
      AND t.relname = $1
      AND NOT ix.indisprimary
      GROUP BY i.relname, ix.indisunique, ix.indisprimary
    `,
      [tableName]
    );

    const indexes: IndexInfo[] = indexResult.rows.map((row) => ({
      index_name: row.index_name,
      // Ensure columns is an array (pg might return it differently)
      columns: Array.isArray(row.columns)
        ? row.columns
        : parsePostgresArray(row.columns),
      is_unique: row.is_unique,
      is_primary: row.is_primary,
    }));

    // Get triggers
    const triggersResult = await client.query(
      `
      SELECT
        trigger_name,
        event_manipulation,
        action_timing,
        action_orientation,
        action_statement
      FROM information_schema.triggers
      WHERE event_object_schema = 'public' AND event_object_table = $1
    `,
      [tableName]
    );

    const triggersMap = new Map<string, TriggerInfo>();

    for (const row of triggersResult.rows) {
      if (!triggersMap.has(row.trigger_name)) {
        // Parse function name from action_statement
        // e.g. EXECUTE FUNCTION my_func()
        const match = row.action_statement.match(
          /EXECUTE (?:FUNCTION|PROCEDURE) (?:public\.)?("?[\w]+"?)\(/i
        );
        const functionName = match ? match[1].replace(/"/g, "") : "";

        triggersMap.set(row.trigger_name, {
          name: row.trigger_name,
          events: [],
          timing: row.action_timing, // BEFORE, AFTER, INSTEAD OF
          orientation: row.action_orientation, // ROW, STATEMENT
          function_name: functionName,
        });
      }
      triggersMap.get(row.trigger_name)!.events.push(row.event_manipulation);
    }
    const triggers = Array.from(triggersMap.values());

    // Get table metadata (RLS)
    const tableMetaResult = await client.query(
      `
      SELECT relrowsecurity
      FROM pg_class c
      JOIN pg_namespace n ON n.oid = c.relnamespace
      WHERE n.nspname = 'public' AND c.relname = $1
      `,
      [tableName]
    );
    const rlsEnabled = tableMetaResult.rows[0]?.relrowsecurity ?? false;

    // Get policies
    const policiesResult = await client.query(
      `
      SELECT
        polname as name,
        polcmd as cmd,
        polroles as roles,
        pg_get_expr(polqual, polrelid) as qual,
        pg_get_expr(polwithcheck, polrelid) as with_check
      FROM pg_policy p
      JOIN pg_class c ON c.oid = p.polrelid
      JOIN pg_namespace n ON n.oid = c.relnamespace
      WHERE n.nspname = 'public' AND c.relname = $1
      `,
      [tableName]
    );

    const policies: PolicyInfo[] = policiesResult.rows.map((row) => ({
      name: row.name,
      cmd: parsePolicyCmd(row.cmd),
      roles: parsePolicyRoles(row.roles, client),
      qual: row.qual,
      with_check: row.with_check,
    }));

    schema.tables.set(tableName, {
      table_name: tableName,
      columns,
      foreignKeys,
      indexes,
      triggers,
      rls_enabled: rlsEnabled,
      policies,
    });
  }

  return schema;
}

function parseFunctionArgs(argsStr: string): { name: string; type: string }[] {
  if (!argsStr) return [];
  // argsStr example: "x integer, y text" or "integer, text"
  // Naive splitting by comma.
  const args = argsStr
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  return args.map((arg) => {
    // Try to split by space to separate name and type
    // But type can have spaces "double precision"
    // And name is optional.
    // If we can't distinguish easily, we might need a better parser or assume the last part is type if we can't find it.
    // Actually, usually it's "name type".
    // If the schema definition has names, we want to match names.
    const parts = arg.split(/\s+/);
    if (parts.length > 1) {
      // Check if first part is a known type? No.
      // Assume first part is name if it's not a reserved type keyword?
      // This is tricky.
      // For now, let's return the whole string as type if we can't be sure,
      // or try to match against schema definition names later?
      // Let's assume standard "name type" format for named args.
      const name = parts[0];
      const type = parts.slice(1).join(" ");
      return { name, type };
    }
    return { name: "", type: arg };
  });
}

function parsePolicyCmd(cmd: string): string {
  switch (cmd) {
    case "r":
      return "SELECT";
    case "a":
      return "INSERT";
    case "w":
      return "UPDATE";
    case "d":
      return "DELETE";
    case "*":
      return "ALL";
    default:
      return cmd;
  }
}

function parsePolicyRoles(roles: any, client: any): string[] {
  // pg_policy.polroles is an oid array (oidvector)
  // In a real implementation we'd need to map OIDs to role names
  // For now, if it's {0} or null/empty it means PUBLIC
  // This is a simplification
  if (!roles || roles === "{0}") return ["public"];
  // We would need to query pg_roles to get names for other OIDs
  return ["authenticated"]; // Placeholder for now
}

// Parse PostgreSQL array string format like {a,b,c} into array
export function parsePostgresArray(str: string): string[] {
  if (!str || str === "{}") return [];

  // Remove the curly braces
  const inner = str.replace(/^\{|\}$/g, "");
  if (!inner) return [];

  const result: string[] = [];
  let current = "";
  let inQuotes = false;
  let escape = false;
  let wasQuoted = false;

  for (let i = 0; i < inner.length; i++) {
    const char = inner[i];

    if (escape) {
      current += char;
      escape = false;
      continue;
    }

    if (char === "\\") {
      escape = true;
      continue;
    }

    if (char === '"') {
      inQuotes = !inQuotes;
      if (inQuotes) {
        wasQuoted = true;
        current = ""; // Discard any preceding whitespace/noise
      }
      continue;
    }

    if (char === "," && !inQuotes) {
      result.push(wasQuoted ? current : current.trim());
      current = "";
      wasQuoted = false;
      continue;
    }

    if (inQuotes) {
      current += char;
    } else {
      // Not in quotes.
      // If we already saw a quoted string for this element, ignore trailing garbage.
      if (!wasQuoted) {
        current += char;
      }
    }
  }

  result.push(wasQuoted ? current : current.trim());
  return result;
}

// Map our schema type names to PostgreSQL data types
export function mapColumnType(type: string): string {
  const typeMap: Record<string, string> = {
    text: "text",
    varchar: "character varying",
    char: "character",
    integer: "integer",
    smallint: "smallint",
    bigint: "bigint",
    serial: "integer",
    bigserial: "bigint",
    real: "real",
    doublePrecision: "double precision",
    numeric: "numeric",
    decimal: "numeric",
    boolean: "boolean",
    timestamp: "timestamp without time zone",
    timestamptz: "timestamp with time zone",
    date: "date",
    time: "time without time zone",
    uuid: "uuid",
    json: "json",
    jsonb: "jsonb",
  };
  return typeMap[type] || type;
}

// Normalize PostgreSQL types for comparison
export function normalizeType(type: string): string {
  const normalized = type.toLowerCase();
  // Handle array types
  if (normalized.startsWith("array")) return normalized;
  // Handle user-defined types (enums)
  if (normalized === "user-defined") return normalized;
  return normalized;
}
