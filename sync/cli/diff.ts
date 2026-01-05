import type {
  DbSchema,
  EnumInfo,
  ForeignKeyInfo,
  FunctionInfo,
  IndexInfo,
  PolicyInfo,
  TableInfo,
  TriggerInfo,
} from "./introspect";
import { mapColumnType, normalizeType } from "./introspect";

export interface ColumnChange {
  column: string;
  changes: {
    type?: { from: string; to: string };
    nullable?: { from: boolean; to: boolean };
    default?: { from: string | null; to: string | null };
    unique?: { from: boolean; to: boolean };
    identity?: { from: boolean; to: boolean };
  };
}

export interface ForeignKeyChange {
  type: "add" | "drop" | "modify";
  constraint_name?: string;
  column: string;
  foreign_table?: string;
  foreign_column?: string;
  on_delete?: string;
}

export interface IndexChange {
  type: "add" | "drop";
  index_name: string;
  columns: string[];
  is_unique: boolean;
}

export interface EnumChange {
  type: "create" | "drop" | "add_value" | "remove_value";
  name: string;
  values?: string[];
  added_values?: string[];
  removed_values?: string[];
}

export interface PolicyChange {
  type: "create" | "drop" | "alter";
  name: string;
  def?: PolicyInfo;
}

export interface TriggerChange {
  type: "create" | "drop" | "alter";
  name: string;
  def?: TriggerInfo;
}

export interface FunctionChange {
  type: "create" | "drop" | "alter";
  name: string;
  def?: FunctionInfo;
}

export interface TableDiff {
  columnsToAdd: string[];
  columnsToDrop: string[];
  columnsToModify: ColumnChange[];
  foreignKeysToAdd: ForeignKeyChange[];
  foreignKeysToDrop: ForeignKeyChange[];
  indexesToAdd: IndexChange[];
  indexesToDrop: IndexChange[];
  policiesToCreate: PolicyChange[];
  policiesToDrop: PolicyChange[];
  triggersToCreate: TriggerChange[];
  triggersToDrop: TriggerChange[];
  rlsChange?: { from: boolean; to: boolean };
}

export interface SchemaDiff {
  tablesToCreate: string[];
  tablesToDrop: string[];
  tableChanges: Map<string, TableDiff>;
  enumChanges: EnumChange[];
  functionChanges: FunctionChange[];
}

export interface LocalSchema {
  tables: Map<string, LocalTableInfo>;
  enums: Map<string, EnumInfo>;
  functions: Map<string, FunctionInfo>;
}

export interface LocalTableInfo {
  table_name: string;
  columns: Map<string, LocalColumnInfo>;
  foreignKeys: ForeignKeyInfo[];
  indexes: IndexInfo[];
  rls_enabled: boolean;
  policies: PolicyInfo[];
  triggers: TriggerInfo[];
}

export interface LocalColumnInfo {
  column_name: string;
  data_type: string;
  is_nullable: boolean;
  column_default: string | null;
  udt_name: string;
  is_primary_key: boolean;
  is_unique: boolean;
  is_identity: boolean;
}

export function extractLocalSchema(schema: Record<string, any>): LocalSchema {
  const localSchema: LocalSchema = {
    tables: new Map(),
    enums: new Map(),
    functions: new Map(),
  };

  // Collect functions
  for (const item of Object.values(schema)) {
    if (
      item &&
      typeof item === "object" &&
      "definition" in item &&
      "language" in item &&
      "returns" in item
    ) {
      // It's a function
      localSchema.functions.set(item.name, {
        name: item.name,
        args: item.args,
        return_type: item.returns,
        language: item.language,
        definition: item.definition,
      });
    }
  }

  // First pass: collect enums
  for (const table of Object.values(schema)) {
    if (!table.__columns) continue; // Skip non-table objects
    for (const column of Object.values(
      table.__columns as Record<string, any>
    )) {
      if (column.__enumDef) {
        localSchema.enums.set(column.__enumDef.name, {
          name: column.__enumDef.name,
          values: [...column.__enumDef.values],
        });
      }
    }
  }

  // Second pass: collect tables
  for (const table of Object.values(schema)) {
    if (!table.__tableName) continue; // Skip non-table objects

    const tableName = table.__tableName;
    const columns = new Map<string, LocalColumnInfo>();
    const foreignKeys: ForeignKeyInfo[] = [];
    const indexes: IndexInfo[] = [];

    // RLS & Policies
    const rls_enabled = table.__rlsEnabled || false;
    const policies: PolicyInfo[] = (table.__policies || []).map((p: any) => ({
      name: p.name,
      cmd: p.on.toUpperCase(),
      roles: p.to.map((r: string) => r.toLowerCase()),
      qual: p.using || null,
      with_check: p.withCheck || null,
    }));

    // Triggers
    const triggers: TriggerInfo[] = (table.__triggers || []).map((t: any) => ({
      name: t.name,
      events: t.events,
      timing: t.event,
      orientation: t.forEach,
      function_name:
        typeof t.function === "string" ? t.function : t.function.name,
    }));

    // Indexes (Composite)
    if (table.__indexes) {
      for (const idx of table.__indexes) {
        if (idx.unique) {
          // It's a unique index (or constraint).
          // We treat it as an index in our LocalSchema structure.
          indexes.push({
            index_name: `${tableName}_${idx.columns.join("_")}_unique_idx`, // Synthetic name, might mismatch DB but we match by columns
            columns: idx.columns,
            is_unique: true,
            is_primary: false,
          });
        } else {
          indexes.push({
            index_name: `${tableName}_${idx.columns.join("_")}_idx`,
            columns: idx.columns,
            is_unique: false,
            is_primary: false,
          });
        }
      }
    }

    for (const [columnName, column] of Object.entries(
      table.__columns as Record<string, any>
    )) {
      let dataType = column.__enumName
        ? "USER-DEFINED"
        : mapColumnType(column.type);

      if (column.__isArray) {
        dataType = dataType + "[]";
      }

      columns.set(columnName, {
        column_name: columnName,
        data_type: dataType,
        is_nullable: !column.notNull && !column.primaryKey,
        column_default: formatDefaultForComparison(column),
        udt_name:
          column.__enumName ||
          mapColumnType(column.type).toLowerCase().replace(/ /g, "_"),
        is_primary_key: !!column.primaryKey,
        is_unique: !!column.unique,
        is_identity: !!column.__isIdentity,
      });

      // Collect foreign keys
      if (column.__references) {
        foreignKeys.push({
          constraint_name: `fk_${tableName}_${columnName}`,
          column_name: columnName,
          foreign_table: column.__references.table,
          foreign_column: column.__references.column,
          on_delete: "CASCADE",
        });
      }

      // Collect indexes from column definitions
      if (column.__index) {
        indexes.push({
          index_name: `${tableName}_${columnName}_idx`,
          columns: [columnName],
          is_unique: false,
          is_primary: false,
        });
      }
      if (column.__uniqueIndex) {
        indexes.push({
          index_name: `${tableName}_${columnName}_unique_idx`,
          columns: [columnName],
          is_unique: true,
          is_primary: false,
        });
      }
    }

    localSchema.tables.set(tableName, {
      table_name: tableName,
      columns,
      foreignKeys,
      indexes,
      rls_enabled,
      policies,
      triggers,
    });
  }

  return localSchema;
}

function formatDefaultForComparison(column: any): string | null {
  if (column.defaultValue !== undefined) {
    const val = column.defaultValue;
    if (typeof val === "string")
      return `'${val}'::${mapColumnType(column.type)}`;
    if (typeof val === "number") return val.toString();
    if (typeof val === "boolean") return val ? "true" : "false";
    return null;
  }

  // Auto-defaults
  if (column.type === "uuid" && column.primaryKey) {
    return "gen_random_uuid()";
  }

  return null;
}

export function computeDiff(
  dbSchema: DbSchema,
  localSchema: LocalSchema
): SchemaDiff {
  const diff: SchemaDiff = {
    tablesToCreate: [],
    tablesToDrop: [],
    tableChanges: new Map(),
    enumChanges: [],
    functionChanges: [],
  };

  // Enum changes
  diffEnums(dbSchema.enums, localSchema.enums, diff.enumChanges);

  // Functions changes
  diffFunctions(
    dbSchema.functions || new Map(),
    localSchema.functions || new Map(),
    diff.functionChanges
  );

  // Tables to create
  for (const tableName of localSchema.tables.keys()) {
    if (!dbSchema.tables.has(tableName)) {
      diff.tablesToCreate.push(tableName);
    }
  }

  // Tables to drop
  for (const tableName of dbSchema.tables.keys()) {
    if (!localSchema.tables.has(tableName)) {
      diff.tablesToDrop.push(tableName);
    }
  }

  // Table changes (for existing tables)
  for (const [tableName, localTable] of localSchema.tables) {
    const dbTable = dbSchema.tables.get(tableName);
    if (!dbTable) continue;

    const tableDiff = diffTable(
      dbTable,
      localTable,
      dbSchema.enums,
      localSchema.enums
    );

    const hasChanges =
      tableDiff.columnsToAdd.length > 0 ||
      tableDiff.columnsToDrop.length > 0 ||
      tableDiff.columnsToModify.length > 0 ||
      tableDiff.foreignKeysToAdd.length > 0 ||
      tableDiff.foreignKeysToDrop.length > 0 ||
      tableDiff.indexesToAdd.length > 0 ||
      tableDiff.indexesToDrop.length > 0 ||
      tableDiff.policiesToCreate.length > 0 ||
      tableDiff.policiesToDrop.length > 0 ||
      tableDiff.triggersToCreate.length > 0 ||
      tableDiff.triggersToDrop.length > 0 ||
      tableDiff.rlsChange !== undefined;

    if (hasChanges) {
      diff.tableChanges.set(tableName, tableDiff);
    }
  }

  return diff;
}

function diffEnums(
  dbEnums: Map<string, EnumInfo>,
  localEnums: Map<string, EnumInfo>,
  changes: EnumChange[]
): void {
  // Enums to create
  for (const [name, localEnum] of localEnums) {
    const dbEnum = dbEnums.get(name);
    if (!dbEnum) {
      changes.push({
        type: "create",
        name,
        values: localEnum.values,
      });
    } else {
      // Check for value changes
      const addedValues = localEnum.values.filter(
        (v) => !dbEnum.values.includes(v)
      );
      const removedValues = dbEnum.values.filter(
        (v) => !localEnum.values.includes(v)
      );

      if (addedValues.length > 0) {
        changes.push({
          type: "add_value",
          name,
          added_values: addedValues,
        });
      }
      if (removedValues.length > 0) {
        changes.push({
          type: "remove_value",
          name,
          removed_values: removedValues,
        });
      }
    }
  }

  // Enums to drop
  for (const name of dbEnums.keys()) {
    if (!localEnums.has(name)) {
      changes.push({
        type: "drop",
        name,
      });
    }
  }
}

function diffTable(
  dbTable: TableInfo,
  localTable: LocalTableInfo,
  dbEnums: Map<string, EnumInfo>,
  localEnums: Map<string, EnumInfo>
): TableDiff {
  const diff: TableDiff = {
    columnsToAdd: [],
    columnsToDrop: [],
    columnsToModify: [],
    foreignKeysToAdd: [],
    foreignKeysToDrop: [],
    indexesToAdd: [],
    indexesToDrop: [],
    policiesToCreate: [],
    policiesToDrop: [],
    triggersToCreate: [],
    triggersToDrop: [],
  };

  // RLS Change
  if (localTable.rls_enabled !== dbTable.rls_enabled) {
    diff.rlsChange = {
      from: !!dbTable.rls_enabled,
      to: !!localTable.rls_enabled,
    };
  }

  // Policy Diffing
  diffPolicies(dbTable.policies || [], localTable.policies || [], diff);

  // Trigger Diffing
  diffTriggers(dbTable.triggers || [], localTable.triggers || [], diff);

  // Columns to add
  for (const columnName of localTable.columns.keys()) {
    if (!dbTable.columns.has(columnName)) {
      diff.columnsToAdd.push(columnName);
    }
  }

  // Columns to drop
  for (const columnName of dbTable.columns.keys()) {
    if (!localTable.columns.has(columnName)) {
      diff.columnsToDrop.push(columnName);
    }
  }

  // Column modifications
  for (const [columnName, localCol] of localTable.columns) {
    const dbCol = dbTable.columns.get(columnName);
    if (!dbCol) continue;

    const changes: ColumnChange["changes"] = {};

    // Type change
    const localType = normalizeTypeForComparison(
      localCol.data_type,
      localCol.udt_name
    );
    const dbType = normalizeTypeForComparison(dbCol.data_type, dbCol.udt_name);

    if (localType !== dbType) {
      changes.type = { from: dbType, to: localType };
    }

    // Nullable change
    if (localCol.is_nullable !== dbCol.is_nullable) {
      changes.nullable = { from: dbCol.is_nullable, to: localCol.is_nullable };
    }

    // Unique change (only for non-primary key columns)
    if (!localCol.is_primary_key && localCol.is_unique !== dbCol.is_unique) {
      changes.unique = { from: dbCol.is_unique, to: localCol.is_unique };
    }

    // Identity change
    if (localCol.is_identity !== dbCol.is_identity) {
      changes.identity = { from: dbCol.is_identity, to: localCol.is_identity };
    }

    if (Object.keys(changes).length > 0) {
      diff.columnsToModify.push({ column: columnName, changes });
    }
  }

  // Foreign key changes
  diffForeignKeys(dbTable.foreignKeys, localTable.foreignKeys, diff);

  // Index changes
  diffIndexes(dbTable.indexes, localTable.indexes, diff, localTable);

  return diff;
}

function normalizeTypeForComparison(dataType: string, udtName: string): string {
  const normalized = dataType.toLowerCase();

  // Handle user-defined types (enums)
  if (normalized === "user-defined") {
    return `enum:${udtName}`;
  }

  // Handle arrays
  if (normalized === "array") {
    // Try to reconstruct type from udt_name (e.g. _text -> text[])
    if (udtName.startsWith("_")) {
      const type = udtName.substring(1);
      // normalize common types
      const normalizedType = normalizeScalarType(type);
      return `${normalizedType}[]`;
    }
    return normalized;
  }

  return normalizeScalarType(normalized);
}

function normalizeScalarType(type: string): string {
  const typeNormalizations: Record<string, string> = {
    "character varying": "varchar",
    character: "char",
    "timestamp without time zone": "timestamp",
    "timestamp with time zone": "timestamptz",
    "time without time zone": "time",
    "double precision": "double",
    int4: "integer",
    int8: "bigint",
    int2: "smallint",
    bool: "boolean",
    float4: "real",
    float8: "double",
  };

  return typeNormalizations[type] || type;
}

function diffForeignKeys(
  dbFks: ForeignKeyInfo[],
  localFks: ForeignKeyInfo[],
  diff: TableDiff
): void {
  // Create lookup by column name
  const dbFkByColumn = new Map((dbFks || []).map((fk) => [fk.column_name, fk]));
  const localFkByColumn = new Map(
    (localFks || []).map((fk) => [fk.column_name, fk])
  );

  // FKs to add
  for (const [column, localFk] of localFkByColumn) {
    const dbFk = dbFkByColumn.get(column);
    if (!dbFk) {
      diff.foreignKeysToAdd.push({
        type: "add",
        column: localFk.column_name,
        foreign_table: localFk.foreign_table,
        foreign_column: localFk.foreign_column,
        on_delete: localFk.on_delete,
      });
    }
  }

  // FKs to drop
  for (const [column, dbFk] of dbFkByColumn) {
    if (!localFkByColumn.has(column)) {
      diff.foreignKeysToDrop.push({
        type: "drop",
        constraint_name: dbFk.constraint_name,
        column: dbFk.column_name,
      });
    }
  }
}

function diffIndexes(
  dbIndexes: IndexInfo[],
  localIndexes: IndexInfo[],
  diff: TableDiff,
  localTable: LocalTableInfo // Pass localTable to check for implicit unique indexes
): void {
  // Create lookup by columns (sorted and joined)
  const indexKey = (idx: IndexInfo) =>
    `${idx.columns.sort().join(",")}:${idx.is_unique}`;

  const dbIndexByKey = new Map(
    (dbIndexes || []).map((idx) => [indexKey(idx), idx])
  );
  const localIndexByKey = new Map(
    (localIndexes || []).map((idx) => [indexKey(idx), idx])
  );

  // Indexes to add
  for (const [key, localIdx] of localIndexByKey) {
    if (!dbIndexByKey.has(key)) {
      diff.indexesToAdd.push({
        type: "add",
        index_name: localIdx.index_name,
        columns: localIdx.columns,
        is_unique: localIdx.is_unique,
      });
    }
  }

  // Indexes to drop
  for (const [key, dbIdx] of dbIndexByKey) {
    if (!localIndexByKey.has(key)) {
      // Check if this index is required by a unique constraint or primary key
      // The introspection already filters out PK indexes, but unique constraints might have implicit indexes
      // If the table has a unique constraint on these exact columns, we shouldn't drop the index individually
      // because dropping the constraint will drop the index (or we can't drop the index without dropping constraint)

      // However, we don't have easy access to constraint names here to check "is this index backing a constraint?".
      // But we know that if we are NOT dropping a unique constraint (which is handled in column modifications),
      // we probably shouldn't try to drop its backing index if it shows up here.

      // If this is a unique index, and the column is marked unique in local schema (which generates a unique constraint),
      // then we should probably NOT drop this index because the unique constraint needs it.

      // But wait, if local schema has `unique: true`, it generates a unique constraint.
      // If DB has `unique: true`, it has a unique constraint.
      // If we are keeping `unique: true`, we keep the constraint, so we keep the index.
      // Why does diff think we are dropping the index?
      // Because `extractLocalSchema` might not be generating an index entry for `unique: true` columns?

      // Let's check `extractLocalSchema`.
      // It generates an index entry if `column.__uniqueIndex` is true.
      // It does NOT generate an index entry if `column.unique` is true (it sets `is_unique` flag on column).

      // BUT `introspectDatabase` returns indexes for unique constraints (unless they are PKs).
      // So `dbIndexes` contains the index for the unique constraint.
      // But `localIndexes` does NOT contain it (because `unique: true` doesn't generate a `__uniqueIndex` entry).
      // So `diffIndexes` sees it in DB but not Local, so it schedules a DROP.

      // Fix: We should check if this DB index corresponds to a column that has `unique: true` in the local schema.
      // If so, we treat it as "present" (implicitly) and don't drop it.

      // Assuming single column index for now (common case for `unique: true`)
      if (dbIdx.columns.length === 1 && dbIdx.is_unique) {
        const colName = dbIdx.columns[0];
        const localCol = localTable.columns.get(colName);
        if (localCol && localCol.is_unique) {
          // This index is likely backing the unique constraint we want to keep.
          // So don't drop it.
          continue;
        }
      }

      diff.indexesToDrop.push({
        type: "drop",
        index_name: dbIdx.index_name,
        columns: dbIdx.columns,
        is_unique: dbIdx.is_unique,
      });
    }
  }
}

function diffPolicies(
  dbPolicies: PolicyInfo[],
  localPolicies: PolicyInfo[],
  diff: TableDiff
): void {
  const dbPolicyMap = new Map(dbPolicies.map((p) => [p.name, p]));
  const localPolicyMap = new Map(localPolicies.map((p) => [p.name, p]));

  // Policies to Create or Update (Drop + Create)
  for (const [name, localPolicy] of localPolicyMap) {
    const dbPolicy = dbPolicyMap.get(name);
    if (!dbPolicy) {
      diff.policiesToCreate.push({
        type: "create",
        name,
        def: localPolicy,
      });
    } else {
      // Check if modified
      const isModified =
        dbPolicy.cmd !== localPolicy.cmd ||
        !arraysEqual(dbPolicy.roles, localPolicy.roles) ||
        dbPolicy.qual !== localPolicy.qual ||
        dbPolicy.with_check !== localPolicy.with_check;

      if (isModified) {
        // PG doesn't support ALTER POLICY easily for everything, so we DROP and CREATE
        diff.policiesToDrop.push({ type: "drop", name });
        diff.policiesToCreate.push({
          type: "create",
          name,
          def: localPolicy,
        });
      }
    }
  }

  // Policies to Drop
  for (const [name] of dbPolicyMap) {
    if (!localPolicyMap.has(name)) {
      diff.policiesToDrop.push({ type: "drop", name });
    }
  }
}

function diffFunctions(
  dbFunctions: Map<string, FunctionInfo>,
  localFunctions: Map<string, FunctionInfo>,
  changes: FunctionChange[]
): void {
  // Functions to Create or Update
  for (const [name, localFunc] of localFunctions) {
    const dbFunc = dbFunctions.get(name);
    if (!dbFunc) {
      changes.push({
        type: "create",
        name,
        def: localFunc,
      });
    } else {
      // Check if modified
      const isModified =
        dbFunc.return_type !== localFunc.return_type ||
        dbFunc.language !== localFunc.language ||
        !argsEqual(dbFunc.args, localFunc.args) ||
        normalizeDefinition(dbFunc.definition) !==
          normalizeDefinition(localFunc.definition);

      if (isModified) {
        // Drop and recreate to handle all cases (return type change, args change)
        changes.push({
          type: "drop",
          name,
          def: dbFunc,
        });
        changes.push({
          type: "create",
          name,
          def: localFunc,
        });
      }
    }
  }

  // Functions to Drop
  for (const [name, dbFunc] of dbFunctions) {
    if (!localFunctions.has(name)) {
      changes.push({
        type: "drop",
        name,
        def: dbFunc,
      });
    }
  }
}

function diffTriggers(
  dbTriggers: TriggerInfo[],
  localTriggers: TriggerInfo[],
  diff: TableDiff
): void {
  const dbTriggerMap = new Map(dbTriggers.map((t) => [t.name, t]));
  const localTriggerMap = new Map(localTriggers.map((t) => [t.name, t]));

  // Triggers to Create or Update (Drop + Create)
  for (const [name, localTrigger] of localTriggerMap) {
    const dbTrigger = dbTriggerMap.get(name);
    if (!dbTrigger) {
      diff.triggersToCreate.push({
        type: "create",
        name,
        def: localTrigger,
      });
    } else {
      // Check if modified
      const isModified =
        dbTrigger.timing !== localTrigger.timing ||
        dbTrigger.orientation !== localTrigger.orientation ||
        dbTrigger.function_name !== localTrigger.function_name ||
        !arraysEqual(dbTrigger.events, localTrigger.events);

      if (isModified) {
        diff.triggersToDrop.push({ type: "drop", name });
        diff.triggersToCreate.push({
          type: "create",
          name,
          def: localTrigger,
        });
      }
    }
  }

  // Triggers to Drop
  for (const [name] of dbTriggerMap) {
    if (!localTriggerMap.has(name)) {
      diff.triggersToDrop.push({ type: "drop", name });
    }
  }
}

function arraysEqual(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false;
  const sortedA = [...a].sort();
  const sortedB = [...b].sort();
  for (let i = 0; i < sortedA.length; i++) {
    if (sortedA[i] !== sortedB[i]) return false;
  }
  return true;
}

function argsEqual(
  a: { name: string; type: string }[],
  b: { name: string; type: string }[]
): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (normalizeType(a[i].type) !== normalizeType(b[i].type)) return false;
  }
  return true;
}

function normalizeDefinition(def: string): string {
  return def.replace(/\s+/g, " ").trim();
}

export function hasDiff(diff: SchemaDiff): boolean {
  return (
    diff.tablesToCreate.length > 0 ||
    diff.tablesToDrop.length > 0 ||
    diff.tableChanges.size > 0 ||
    (diff.enumChanges?.length || 0) > 0 ||
    (diff.functionChanges?.length || 0) > 0
  );
}
