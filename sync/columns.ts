// Column definitions and builders

import type { ColumnBaseType, ColumnOptions } from "./types";

export class ColumnDefinition<
  T = any,
  TType extends ColumnBaseType = ColumnBaseType,
> {
  type: TType;
  notNull?: boolean;
  primaryKey?: boolean;
  defaultValue?: T;
  unique?: boolean;
  length?: number;
  precision?: number;
  scale?: number;
  __isIdentity?: boolean;
  __references?: {
    table: string;
    column: string;
  };
  __defaultFn?: () => T;
  __enumName?: string;
  __index?: boolean;
  __uniqueIndex?: boolean;
  __isArray?: boolean;

  constructor(type: TType, options?: ColumnOptions<T>) {
    this.type = type;
    this.notNull = options?.notNull;
    this.primaryKey = options?.primaryKey;
    this.defaultValue = options?.defaultValue;
    this.unique = options?.unique;
    this.length = options?.length;
    this.precision = options?.precision;
    this.scale = options?.scale;
    this.__isIdentity = options?.generatedAlwaysAsIdentity;
  }

  generatedAlwaysAsIdentity(): this {
    this.__isIdentity = true;
    return this;
  }

  references<TTable>(getColumn: () => ColumnDefinition): this {
    const columnRef = getColumn();
    const tableName = (columnRef as any).__tableName;
    const columnName = (columnRef as any).__columnName;

    if (!tableName || !columnName) {
      throw new Error(
        "Invalid column reference. Make sure you're referencing a column from a table definition."
      );
    }

    this.__references = {
      table: tableName,
      column: columnName,
    };
    return this;
  }

  index(): this {
    this.__index = true;
    return this;
  }

  uniqueIndex(): this {
    this.__uniqueIndex = true;
    return this;
  }

  array(): this {
    this.__isArray = true;
    return this;
  }

  $default(fn: () => T): this {
    this.__defaultFn = fn;
    return this;
  }

  default(value: T): this {
    this.defaultValue = value;
    return this;
  }
}

// Text types
export function text(options?: Omit<ColumnOptions<string>, "length">) {
  return new ColumnDefinition<string, "text">("text", options);
}

export function varchar(
  options?: ColumnOptions<string> & { length?: number }
): ColumnDefinition<string, "varchar">;
export function varchar(
  name: string,
  options?: ColumnOptions<string> & { length?: number }
): ColumnDefinition<string, "varchar">;
export function varchar(
  nameOrOptions?: string | (ColumnOptions<string> & { length?: number }),
  options?: ColumnOptions<string> & { length?: number }
): ColumnDefinition<string, "varchar"> {
  const opts = typeof nameOrOptions === "string" ? options : nameOrOptions;
  return new ColumnDefinition<string, "varchar">("varchar", opts);
}

export function char(
  options?: ColumnOptions<string> & { length?: number }
): ColumnDefinition<string, "char">;
export function char(
  name: string,
  options?: ColumnOptions<string> & { length?: number }
): ColumnDefinition<string, "char">;
export function char(
  nameOrOptions?: string | (ColumnOptions<string> & { length?: number }),
  options?: ColumnOptions<string> & { length?: number }
): ColumnDefinition<string, "char"> {
  const opts = typeof nameOrOptions === "string" ? options : nameOrOptions;
  return new ColumnDefinition<string, "char">("char", opts);
}

// Numeric types
export function integer(options?: ColumnOptions<number>) {
  return new ColumnDefinition<number, "integer">("integer", options);
}

export function smallint(options?: ColumnOptions<number>) {
  return new ColumnDefinition<number, "smallint">("smallint", options);
}

export function bigint(options?: ColumnOptions<bigint>) {
  return new ColumnDefinition<bigint, "bigint">("bigint", options);
}

export function serial(
  options?: Omit<
    ColumnOptions<number>,
    "defaultValue" | "generatedAlwaysAsIdentity"
  >
) {
  return new ColumnDefinition<number, "serial">("serial", {
    ...options,
    generatedAlwaysAsIdentity: true,
  });
}

export function bigserial(
  options?: Omit<
    ColumnOptions<bigint>,
    "defaultValue" | "generatedAlwaysAsIdentity"
  >
) {
  return new ColumnDefinition<bigint, "bigserial">("bigserial", {
    ...options,
    generatedAlwaysAsIdentity: true,
  });
}

export function real(options?: ColumnOptions<number>) {
  return new ColumnDefinition<number, "real">("real", options);
}

export function doublePrecision(options?: ColumnOptions<number>) {
  return new ColumnDefinition<number, "doublePrecision">(
    "doublePrecision",
    options
  );
}

export function numeric(
  options?: ColumnOptions<string> & { precision?: number; scale?: number }
) {
  return new ColumnDefinition<string, "numeric">("numeric", options);
}

export function decimal(
  options?: ColumnOptions<string> & { precision?: number; scale?: number }
) {
  return new ColumnDefinition<string, "decimal">("decimal", options);
}

// Boolean type
export function boolean(options?: ColumnOptions<boolean>) {
  return new ColumnDefinition<boolean, "boolean">("boolean", options);
}

// Date/Time types
export function timestamp(options?: ColumnOptions<string | Date>) {
  return new ColumnDefinition<string | Date, "timestamp">("timestamp", options);
}

export function timestamptz(options?: ColumnOptions<string | Date>) {
  return new ColumnDefinition<string | Date, "timestamptz">(
    "timestamptz",
    options
  );
}

export function date(options?: ColumnOptions<string | Date>) {
  return new ColumnDefinition<string | Date, "date">("date", options);
}

export function time(options?: ColumnOptions<string>) {
  return new ColumnDefinition<string, "time">("time", options);
}

// UUID type
export function uuid(options?: ColumnOptions<string>) {
  return new ColumnDefinition<string, "uuid">("uuid", options);
}

// JSON types
export function json<T = any>(options?: ColumnOptions<T>) {
  return new ColumnDefinition<T, "json">("json", options);
}

export function jsonb<T = any>(options?: ColumnOptions<T>) {
  return new ColumnDefinition<T, "jsonb">("jsonb", options);
}

// Type inference helpers
type InferColumnType<T extends ColumnDefinition> =
  T extends ColumnDefinition<infer U>
    ? T["__isArray"] extends true
      ? U[]
      : U
    : never;

export type { InferColumnType };
