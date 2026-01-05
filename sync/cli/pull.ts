import fs from "fs";
import path from "path";
import pg from "pg";
import { introspectDatabase } from "./introspect";
import { generateTypeScript } from "./schema-generator";

const { Client } = pg;

// Load environment variables from .env files
function loadEnvFiles() {
  const cwd = process.cwd();
  const envFiles = [
    path.join(cwd, ".env.local"),
    path.join(cwd, ".env"),
  ];

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

async function run() {
  const args = process.argv.slice(2);
  if (args.length < 1) {
    console.error("Usage: supabase-schema pull <schema-file>");
    console.error("\nEnvironment variables:");
    console.error("  DATABASE_URL - PostgreSQL connection string");
    process.exit(1);
  }

  const schemaPath = args[0];
  const absolutePath = path.resolve(process.cwd(), schemaPath);

  const connectionString = process.env.DATABASE_URL;
  if (!connectionString) {
    console.error("DATABASE_URL environment variable is required");
    console.error("\nExample: DATABASE_URL=postgres://user:pass@host:5432/db npx supabase-schema pull schema.ts");
    process.exit(1);
  }

  const client = new Client({ connectionString });

  try {
    // Connect to database
    console.log("Connecting to database...");
    await client.connect();

    // Introspect database
    console.log("Fetching database schema...");
    const dbSchema = await introspectDatabase(client);

    // Generate TypeScript code
    console.log("Generating schema file...");
    const tsCode = generateTypeScript(dbSchema);

    // Write to file
    fs.writeFileSync(absolutePath, tsCode);
    console.log(`\n✓ Schema pulled successfully to ${schemaPath}`);

  } catch (error) {
    console.error("\nError:", error);
    process.exit(1);
  } finally {
    await client.end();
  }
}

run();
