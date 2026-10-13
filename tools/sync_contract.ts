/**
 * @module
 * Copies the engine's generated contract examples into the client's fixtures, byte for byte.
 *
 *   deno run --allow-read --allow-write tools/sync_contract.ts <engine checkout>
 *   deno run --allow-read tools/sync_contract.ts <engine checkout> --check
 *
 * Generate them first in the engine (`deno task contract:gen`). `--check` exits 1 when the fixtures differ
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

if (check && changed.length > 0) {
  console.error(
    `fixtures differ from the engine's examples: ${changed.join(", ")}`,
  );
  Deno.exit(1);
}
console.log(
  changed.length > 0 ? `synced ${changed.join(", ")}` : "fixtures are current",
);
