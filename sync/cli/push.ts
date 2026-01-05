import fs from "fs";
import path from "path";
import pg from "pg";
import { createInterface } from "readline";
import { pathToFileURL } from "url";
import {
  computeDiff,
  extractLocalSchema,
  hasDiff,
  type SchemaDiff,
  type TableDiff,
} from "./diff";
import { introspectDatabase } from "./introspect";
import { generateSql, type ColumnRenames } from "./sql-generator";

const { Client } = pg;

// Load environment variables from .env files
function loadEnvFiles() {
  const cwd = process.cwd();
  const envFiles = [path.join(cwd, ".env.local"), path.join(cwd, ".env")];

  for (const envFile of envFiles) {
    if (fs.existsSync(envFile)) {
      const content = fs.readFileSync(envFile, "utf-8");
      for (const line of content.split("\n")) {
        const trimmed = line.trim();
        if (!trimmed || trimmed.startsWith("#")) continue;
        const eqIndex = trimmed.indexOf("=");
        if (eqIndex === -1) continue;
        const key = trimmed.slice(0, eqIndex);
        const value = trimmed.slice(eqIndex + 1);
        if (!process.env[key]) {
          process.env[key] = value;
        }
      }
    }
  }
}

loadEnvFiles();

async function prompt(question: string): Promise<string> {
  const rl = createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  return new Promise((resolve) => {
    rl.question(question, (answer) => {
      rl.close();
      resolve(answer.trim().toLowerCase());
    });
  });
}

function printDiff(diff: SchemaDiff) {
  console.log("\n=== Schema Diff ===\n");

  // Enum changes
  for (const enumChange of diff.enumChanges) {
    if (enumChange.type === "create") {
      console.log(`Enum to CREATE: ${enumChange.name}`);
      console.log(`  Values: ${enumChange.values?.join(", ")}`);
    } else if (enumChange.type === "drop") {
      console.log(`Enum to DROP: ${enumChange.name}`);
    } else if (enumChange.type === "add_value") {
      console.log(
        `Enum "${enumChange.name}" - values to ADD: ${enumChange.added_values?.join(", ")}`
      );
    } else if (enumChange.type === "remove_value") {
      console.log(
        `⚠️  Enum "${enumChange.name}" - values to REMOVE: ${enumChange.removed_values?.join(", ")}`
      );
      console.log(
        `   (Note: PostgreSQL doesn't support removing enum values directly)`
      );
    }
  }

  // Tables to create
  if (diff.tablesToCreate.length > 0) {
    console.log("\nTables to CREATE:");
    for (const t of diff.tablesToCreate) console.log(`  + ${t}`);
  }

  // Tables to drop
  if (diff.tablesToDrop.length > 0) {
    console.log("\nTables to DROP:");
    for (const t of diff.tablesToDrop) console.log(`  - ${t}`);
  }

  // Table modifications
  for (const [tableName, tableDiff] of diff.tableChanges) {
    console.log(`\nTable "${tableName}":`);
    printTableDiff(tableDiff);
  }
}

function printTableDiff(tableDiff: TableDiff) {
  if (tableDiff.columnsToAdd.length > 0) {
    console.log("  Columns to ADD:");
    for (const c of tableDiff.columnsToAdd) console.log(`    + ${c}`);
  }

  if (tableDiff.columnsToDrop.length > 0) {
    console.log("  Columns to DROP:");
    for (const c of tableDiff.columnsToDrop) console.log(`    - ${c}`);
  }

  if (tableDiff.columnsToModify.length > 0) {
    console.log("  Columns to MODIFY:");
    for (const mod of tableDiff.columnsToModify) {
      const changes: string[] = [];
      if (mod.changes.type) {
        changes.push(`type: ${mod.changes.type.from} → ${mod.changes.type.to}`);
      }
      if (mod.changes.nullable) {
        changes.push(
          `nullable: ${mod.changes.nullable.from} → ${mod.changes.nullable.to}`
        );
      }
      if (mod.changes.unique) {
        changes.push(
          `unique: ${mod.changes.unique.from} → ${mod.changes.unique.to}`
        );
      }
      console.log(`    ~ ${mod.column} (${changes.join(", ")})`);
    }
  }

  if (tableDiff.foreignKeysToAdd.length > 0) {
    console.log("  Foreign keys to ADD:");
    for (const fk of tableDiff.foreignKeysToAdd) {
      console.log(
        `    + ${fk.column} → ${fk.foreign_table}(${fk.foreign_column})`
      );
    }
  }

  if (tableDiff.foreignKeysToDrop.length > 0) {
    console.log("  Foreign keys to DROP:");
    for (const fk of tableDiff.foreignKeysToDrop) {
      console.log(`    - ${fk.column}`);
    }
  }

  if (tableDiff.indexesToAdd.length > 0) {
    console.log("  Indexes to ADD:");
    for (const idx of tableDiff.indexesToAdd) {
      const unique = idx.is_unique ? " (unique)" : "";
      console.log(
        `    + ${idx.index_name} on (${idx.columns.join(", ")})${unique}`
      );
    }
  }

  if (tableDiff.indexesToDrop.length > 0) {
    console.log("  Indexes to DROP:");
    for (const idx of tableDiff.indexesToDrop) {
      console.log(`    - ${idx.index_name}`);
    }
  }

  // RLS Changes
  if (tableDiff.rlsChange) {
    if (tableDiff.rlsChange.to) {
      console.log("  RLS: ENABLED");
    } else {
      console.log("  RLS: DISABLED");
    }
  }

  // Policies
  if (tableDiff.policiesToCreate.length > 0) {
    console.log("  Policies to CREATE/UPDATE:");
    for (const p of tableDiff.policiesToCreate) {
      console.log(`    + ${p.name}`);
    }
  }
  if (tableDiff.policiesToDrop.length > 0) {
    console.log("  Policies to DROP:");
    for (const p of tableDiff.policiesToDrop) {
      console.log(`    - ${p.name}`);
    }
  }
}

async function handleDestructiveChanges(
  diff: SchemaDiff
): Promise<ColumnRenames> {
  const renames: ColumnRenames = {};

  // Ask about dropping tables
  if (diff.tablesToDrop.length > 0) {
    console.log("\n⚠️  WARNING: The following tables will be DROPPED:");
    for (const t of diff.tablesToDrop) console.log(`    ${t}`);

    const answer = await prompt(
      "\nDo you want to drop these tables? (yes/no): "
    );
    if (answer !== "yes" && answer !== "y") {
      console.log("Removing table drops from migration...");
      diff.tablesToDrop.length = 0;
    }
  }

  // Ask about dropping enums
  const enumsToDrop = diff.enumChanges.filter((e) => e.type === "drop");
  if (enumsToDrop.length > 0) {
    console.log("\n⚠️  WARNING: The following enums will be DROPPED:");
    for (const e of enumsToDrop) console.log(`    ${e.name}`);

    const answer = await prompt(
      "\nDo you want to drop these enums? (yes/no): "
    );
    if (answer !== "yes" && answer !== "y") {
      console.log("Removing enum drops from migration...");
      diff.enumChanges = diff.enumChanges.filter((e) => e.type !== "drop");
    }
  }

  // Ask about column changes per table
  for (const [tableName, tableDiff] of diff.tableChanges) {
    const droppedCols = [...tableDiff.columnsToDrop];
    const addedCols = [...tableDiff.columnsToAdd];

    if (droppedCols.length > 0 && addedCols.length > 0) {
      // Potential renames
      console.log(
        `\n⚠️  Table "${tableName}" has columns being removed and added:`
      );
      console.log(`   Removed: ${droppedCols.join(", ")}`);
      console.log(`   Added: ${addedCols.join(", ")}`);

      const usedNewCols = new Set<string>();

      for (const oldCol of droppedCols) {
        const availableNewCols = addedCols.filter((c) => !usedNewCols.has(c));

        if (availableNewCols.length === 0) {
          const confirmDrop = await prompt(
            `\nDrop column "${oldCol}"? (yes/no): `
          );
          if (confirmDrop !== "yes" && confirmDrop !== "y") {
            const idx = tableDiff.columnsToDrop.indexOf(oldCol);
            if (idx > -1) tableDiff.columnsToDrop.splice(idx, 1);
          }
          continue;
        }

        console.log(
          `\nColumn "${oldCol}" is being removed from "${tableName}".`
        );
        console.log("Options:");
        console.log("  1. Drop the column (data will be lost)");

        for (let i = 0; i < availableNewCols.length; i++) {
          console.log(`  ${i + 2}. Rename to "${availableNewCols[i]}"`);
        }

        const answer = await prompt(
          `Choose option (1-${availableNewCols.length + 1}): `
        );
        const choice = parseInt(answer);

        if (choice > 1 && choice <= availableNewCols.length + 1) {
          const newCol = availableNewCols[choice - 2];
          if (!renames[tableName]) {
            renames[tableName] = new Map();
          }
          renames[tableName].set(oldCol, newCol);
          usedNewCols.add(newCol);
          console.log(`  → Will rename "${oldCol}" to "${newCol}"`);
        } else {
          const confirmDrop = await prompt(
            `Confirm drop column "${oldCol}"? (yes/no): `
          );
          if (confirmDrop !== "yes" && confirmDrop !== "y") {
            const idx = tableDiff.columnsToDrop.indexOf(oldCol);
            if (idx > -1) tableDiff.columnsToDrop.splice(idx, 1);
          }
        }
      }
    } else if (droppedCols.length > 0) {
      console.log(
        `\n⚠️  The following columns will be DROPPED from "${tableName}":`
      );
      for (const c of droppedCols) console.log(`    ${c}`);

      const answer = await prompt(
        "\nDo you want to drop these columns? (yes/no): "
      );
      if (answer !== "yes" && answer !== "y") {
        console.log("Removing column drops from migration...");
        tableDiff.columnsToDrop.length = 0;
      }
    }

    // Ask about dropping indexes
    if (tableDiff.indexesToDrop.length > 0) {
      console.log(
        `\n⚠️  The following indexes will be DROPPED from "${tableName}":`
      );
      for (const idx of tableDiff.indexesToDrop) {
        console.log(`    ${idx.index_name}`);
      }

      const answer = await prompt(
        "\nDo you want to drop these indexes? (yes/no): "
      );
      if (answer !== "yes" && answer !== "y") {
        console.log("Removing index drops from migration...");
        tableDiff.indexesToDrop.length = 0;
      }
    }

    // Ask about dropping foreign keys
    if (tableDiff.foreignKeysToDrop.length > 0) {
      console.log(
        `\n⚠️  The following foreign keys will be DROPPED from "${tableName}":`
      );
      for (const fk of tableDiff.foreignKeysToDrop) {
        console.log(`    ${fk.constraint_name || fk.column}`);
      }

      const answer = await prompt(
        "\nDo you want to drop these foreign keys? (yes/no): "
      );
      if (answer !== "yes" && answer !== "y") {
        console.log("Removing foreign key drops from migration...");
        tableDiff.foreignKeysToDrop.length = 0;
      }
    }
  }

  return renames;
}

async function run() {
  const args = process.argv.slice(2);
  if (args.length < 1) {
    console.error("Usage: supabase-schema push <schema-file>");
    console.error("\nEnvironment variables:");
    console.error("  DATABASE_URL - PostgreSQL connection string");
    process.exit(1);
  }

  const schemaPath = args[0];
  const absolutePath = path.resolve(process.cwd(), schemaPath);

  if (!fs.existsSync(absolutePath)) {
    console.error(`Schema file not found at: ${absolutePath}`);
    process.exit(1);
  }

  const connectionString = process.env.DATABASE_URL;
  if (!connectionString) {
    console.error("DATABASE_URL environment variable is required");
    console.error(
      "\nExample: DATABASE_URL=postgres://user:pass@host:5432/db npx supabase-schema push schema.ts"
    );
    process.exit(1);
  }

  const client = new Client({ connectionString });

  try {
    // Load local schema
    const module = await import(pathToFileURL(absolutePath).href);
    const schema = module.schema || module.default;

    if (!schema) {
      console.error(`No 'schema' export found in ${absolutePath}`);
      process.exit(1);
    }

    // Connect to database
    console.log("Connecting to database...");
    await client.connect();

    // Introspect database
    console.log("Fetching database schema...");
    const dbSchema = await introspectDatabase(client);

    // Extract local schema
    const localSchema = extractLocalSchema(schema);

    // Compute diff
    const diff = computeDiff(dbSchema, localSchema);

    // Check if there are any changes
    if (!hasDiff(diff)) {
      console.log("\n✓ Database is in sync with schema. No changes needed.");
      await client.end();
      return;
    }

    // Print diff
    printDiff(diff);

    // Handle destructive operations
    const renames = await handleDestructiveChanges(diff);

    // Generate SQL
    const sql = generateSql(diff, localSchema, schema, renames, dbSchema);

    if (sql.length === 0) {
      console.log("\n✓ No changes to apply after user selections.");
      await client.end();
      return;
    }

    // Show SQL preview
    console.log("\n=== SQL to execute ===\n");
    console.log(sql.join("\n\n"));

    // Confirm execution
    const confirm = await prompt("\nExecute these changes? (yes/no): ");
    if (confirm !== "yes" && confirm !== "y") {
      console.log("Aborted.");
      await client.end();
      return;
    }

    // Execute SQL
    console.log("\nApplying changes...");

    try {
      await client.query("BEGIN");

      for (const statement of sql) {
        try {
          await client.query(statement);
        } catch (error: any) {
          console.error(`\nError executing SQL:\n${statement}\n`);
          console.error(error.message);
          throw error;
        }
      }

      await client.query("COMMIT");
      console.log("\n✓ Schema pushed successfully!");
    } catch (error) {
      try {
        await client.query("ROLLBACK");
        console.error("\nTransaction rolled back.");
      } catch (rollbackError) {
        console.error("\nError rolling back transaction:", rollbackError);
      }
      throw error;
    }
  } catch (error) {
    console.error("\nError:", error);
    process.exit(1);
  } finally {
    await client.end();
  }
}

run();
