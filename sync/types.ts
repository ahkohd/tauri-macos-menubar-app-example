// Core types for schema definitions
import type { ColumnDefinition } from "./columns";

export type ColumnBaseType =
  | "text"
  | "varchar"
  | "char"
  | "integer"
  | "smallint"
  | "bigint"
  | "serial"
  | "bigserial"
  | "boolean"
  | "timestamp"
  | "timestamptz"
  | "date"
  | "time"
  | "uuid"
  | "json"
  | "jsonb"
  | "real"
  | "doublePrecision"
  | "numeric"
  | "decimal";

export interface ColumnOptions<T = any> {
  notNull?: boolean;
  primaryKey?: boolean;
  defaultValue?: T;
  unique?: boolean;
  length?: number;
  precision?: number;
  scale?: number;
  generatedAlwaysAsIdentity?: boolean;
}

export interface TableSchema<T = any, C = any> {
  __tableName: string;
  __columns: Record<keyof T, ColumnDefinition>;
  __indexes?: IndexDefinition[];
  __enums?: Record<string, EnumDefinition>;
  __rlsEnabled?: boolean;
  __policies?: PolicyDefinition[];
  __triggers?: TriggerDefinition[];
  enableRLS: () => TableSchema<T, C> & C;
  policy: (name: string, definition: PolicyOptions) => TableSchema<T, C> & C;
  trigger: (name: string, definition: TriggerOptions) => TableSchema<T, C> & C;
  index: (columns: (keyof T)[]) => TableSchema<T, C> & C;
  unique: (columns: (keyof T)[]) => TableSchema<T, C> & C;
}

export interface PolicyDefinition {
  name: string;
  on: "select" | "insert" | "update" | "delete" | "all";
  to: string[];
  using?: string | FunctionDefinition;
  withCheck?: string | FunctionDefinition;
}

export interface PolicyOptions {
  for: "select" | "insert" | "update" | "delete" | "all";
  to?: string[];
  using?: string | FunctionDefinition;
  withCheck?: string | FunctionDefinition;
}

export interface TriggerDefinition {
  name: string;
  event: "BEFORE" | "AFTER" | "INSTEAD OF";
  events: ("INSERT" | "UPDATE" | "DELETE" | "TRUNCATE")[];
  forEach: "ROW" | "STATEMENT";
  function: string | FunctionDefinition;
}

export interface TriggerOptions {
  events: ("INSERT" | "UPDATE" | "DELETE" | "TRUNCATE")[];
  when: "BEFORE" | "AFTER" | "INSTEAD OF";
  forEach?: "ROW" | "STATEMENT"; // Default to ROW usually
  call: string | FunctionDefinition;
}

export interface FunctionDefinition {
  name: string;
  args: { name: string; type: string }[];
  returns: string;
  language: string;
  definition: string;
}

export interface EnumDefinition {
  name: string;
  values: readonly string[];
}

export interface IndexDefinition {
  name?: string;
  columns: string[];
  unique?: boolean;
}

// Type inference helpers
export type InferTableType<T extends TableSchema> =
  T extends TableSchema<infer R> ? R : never;

export type Schema = Record<
  string,
  TableSchema<any> | FunctionDefinition | EnumDefinition
>;

export type TableNames<S extends Schema> = {
  [K in keyof S]: S[K] extends TableSchema<any> ? K : never;
}[keyof S] &
  string;

export type TableRow<S extends Schema, T extends TableNames<S>> =
  S[T] extends TableSchema<infer R> ? R : never;
