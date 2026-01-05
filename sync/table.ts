// Table definitions

import type { ColumnDefinition } from "./columns";
import type { PolicyOptions, TableSchema, TriggerOptions } from "./types";

export function table<T extends Record<string, ColumnDefinition>>(
  name: string,
  columns: T
): TableSchema<{ [K in keyof T]: InferColumnType<T[K]> }, T> & T {
  // Create proxies for each column that include table and column name metadata
  const columnProxies: any = {};

  for (const [key, column] of Object.entries(columns)) {
    // Add metadata to the column for reference tracking
    (column as any).__tableName = name;
    (column as any).__columnName = key;
    columnProxies[key] = column;
  }

  const tableObj = {
    __tableName: name,
    __columns: columns,
    __rlsEnabled: false,
    __policies: [],
    __triggers: [],
    __indexes: [],
    enableRLS() {
      this.__rlsEnabled = true;
      return this;
    },
    policy(name: string, definition: PolicyOptions) {
      this.__policies.push({
        name,
        on: definition.for,
        to: definition.to || ["public"],
        using: definition.using,
        withCheck: definition.withCheck,
      });
      return this;
    },
    trigger(name: string, definition: TriggerOptions) {
      this.__triggers.push({
        name,
        event: definition.when,
        events: definition.events,
        forEach: definition.forEach || "ROW",
        function: definition.call,
      });
      return this;
    },
    index(columns: string[]) {
      this.__indexes.push({
        columns,
        unique: false,
        name: undefined, // ensure name property exists
      });
      return this;
    },
    unique(columns: string[]) {
      this.__indexes.push({
        columns,
        unique: true,
        name: undefined, // ensure name property exists
      });
      return this;
    },
    ...columnProxies,
  } as any;

  return tableObj;
}

// Type inference helper
type InferColumnType<T extends ColumnDefinition> =
  T extends ColumnDefinition<infer U> ? U : never;
