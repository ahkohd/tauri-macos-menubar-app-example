// Enum definitions

import { ColumnDefinition } from "./columns"
import type { EnumDefinition } from "./types"

export class EnumColumn<T extends readonly string[]> extends ColumnDefinition<T[number], "varchar"> {
  __enumDef: EnumDefinition

  constructor(enumDef: EnumDefinition, options?: { notNull?: boolean; defaultValue?: T[number] }) {
    super("varchar", options)
    this.__enumDef = enumDef
    this.__enumName = enumDef.name
  }

  default(value: T[number]): this {
    this.defaultValue = value
    return this
  }
}

export function enumType<T extends readonly string[]>(name: string, values: T) {
  const enumDef: EnumDefinition = { name, values }

  return (options?: { notNull?: boolean; defaultValue?: T[number] }) => {
    return new EnumColumn<T>(enumDef, options)
  }
}
