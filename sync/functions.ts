
import { FunctionDefinition } from "./types";

export interface FunctionOptions {
  args?: { name: string; type: string }[];
  returns: string;
  language?: string;
  definition: string;
}

export function pgFunction(
  name: string,
  options: FunctionOptions
): FunctionDefinition {
  return {
    name,
    args: options.args || [],
    returns: options.returns,
    language: options.language || "plpgsql",
    definition: options.definition,
  };
}

// Aliases
export const createFunction = pgFunction;
export const func = pgFunction;
