/**
 * @module
 * Copies the engine's generated contract examples and its schema into the client, byte for byte, and pins the
 * schema's hash in the wire test.
 *
 *   deno run --allow-read --allow-write tools/sync_contract.ts <engine checkout>
 *   deno run --allow-read tools/sync_contract.ts <engine checkout> --check
 *
 * Generate them first in the engine (`deno task contract:gen`). `--check` exits 1 when the fixtures, the schema or the hash differ
 * from the engine's files; a file the engine no longer has is removed from the fixtures.
 */
import { toFileUrl } from "jsr:@std/path@1.1.2";

const [engine, flag] = Deno.args;
if (!engine || (flag !== undefined && flag !== "--check")) {
  console.error("usage: sync_contract.ts <engine checkout> [--check]");
  Deno.exit(2);
}
const check = flag === "--check";
const source = new URL(
  `file://${await Deno.realPath(
    engine,
  )}/docs/schema/engine-client/0.2.examples/`,
);
const target = new URL(
  "../crates/core/tests/fixtures/0.2.examples/",
  import.meta.url,
);

const names = async (dir: URL) => {
  try {
    return (await Array.fromAsync(Deno.readDir(dir))).map((entry) => entry.name)
      .filter((name) => name.endsWith(".json")).sort();
  } catch (error) {
    if (error instanceof Deno.errors.NotFound) return [];
    throw error;
  }
};
const wanted = await names(source);
if (wanted.length === 0) {
  console.error(`no examples under ${source.pathname}`);
  Deno.exit(2);
}

const changed: string[] = [];
for (const name of wanted) {
  const text = await Deno.readTextFile(new URL(name, source));
  const current = await Deno.readTextFile(new URL(name, target)).catch(() =>
    undefined
  );
  if (current === text) continue;
  changed.push(name);
  if (!check) {
    await Deno.mkdir(target, { recursive: true });
    await Deno.writeTextFile(new URL(name, target), text);
  }
}
for (
  const name of (await names(target)).filter((name) => !wanted.includes(name))
) {
  changed.push(name);
  if (!check) await Deno.remove(new URL(name, target));
}

// The engine hashes the schema it was configured with and refuses a client that vendors another one, so the
// schema and the hash the wire test pins move with the examples.
const schemaSource = new URL(
  "../0.2.schema.json",
  source,
);
const schemaTarget = new URL(
  "../crates/core/schema/0.2.schema.json",
  import.meta.url,
);
const hashTest = new URL("../crates/core/tests/wire_02.rs", import.meta.url);
const schema = await Deno.readFile(schemaSource);
const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", schema));
const hash = `sha256:${
  [...digest.slice(0, 8)].map((b) => b.toString(16).padStart(2, "0")).join("")
}`;
const vendored = await Deno.readFile(schemaTarget).catch(() => undefined);
if (
  vendored === undefined || vendored.length !== schema.length ||
  vendored.some((byte, i) => byte !== schema[i])
) {
  changed.push("0.2.schema.json");
  if (!check) await Deno.writeFile(schemaTarget, schema);
}
const pinned = /protocol::SCHEMA_HASH, "sha256:[0-9a-f]{16}"/;
const test = await Deno.readTextFile(hashTest);
if (!test.includes(`protocol::SCHEMA_HASH, "${hash}"`)) {
  changed.push("wire_02.rs");
  if (!check) {
    await Deno.writeTextFile(
      hashTest,
      test.replace(pinned, `protocol::SCHEMA_HASH, "${hash}"`),
    );
  }
}

if (check && changed.length > 0) {
  console.error(
    `client differs from the engine: ${changed.join(", ")}`,
  );
  Deno.exit(1);
}
console.log(
  changed.length > 0 ? `synced ${changed.join(", ")}` : "fixtures are current",
);
