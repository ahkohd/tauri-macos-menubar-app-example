import fs from "fs";
import path from "path";
import { pathToFileURL } from "url";
import { schemaToPostgres } from "../to-postgres";

async function run() {
  const args = process.argv.slice(2);
  if (args.length < 1) {
    console.error("Please provide a path to the schema file");
    process.exit(1);
  }

  const schemaPath = args[0];
  const absolutePath = path.resolve(process.cwd(), schemaPath);

  if (!fs.existsSync(absolutePath)) {
    console.error(`Schema file not found at: ${absolutePath}`);
    process.exit(1);
  }

  try {
    // Dynamic import to load the user's schema file
    // We use pathToFileURL to properly handle Windows paths and ESM imports
    const module = await import(pathToFileURL(absolutePath).href);
    const schema = module.schema || module.default;

    if (!schema) {
      console.error(`No 'schema' export found in ${absolutePath}`);
      console.error(
        "Please export your schema object as 'schema' or default export."
      );
      process.exit(1);
    }

    const sql = schemaToPostgres(schema, {
      includeDrop: false,
    });

    console.log("-- Generated SQL from schema definition");
    console.log("-- Run this in your Supabase SQL editor or via migration\n");
    console.log(sql);
  } catch (error) {
    console.error("Error generating schema:", error);
    process.exit(1);
  }
}

run();
