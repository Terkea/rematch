import { readdir, readFile } from "node:fs/promises";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { lintFile } from "./ban-rules.js";

export type BanFinding = { rule: string; path: string; line: number; match: string };

const SCAN_DIRS = ["apps/web/src", "packages/ui/src"];
const EXTENSIONS = /\.(?:tsx?|css)$/;
const SKIP_DIRS = new Set(["node_modules", ".next", "dist"]);
// Lab pages 404 in production (labEnabled guard) and exist to try off-token looks.
const LAB_EXEMPT = /(^|\/)apps\/web\/src\/app\/lab\//;

async function walk(dir: string, out: string[]): Promise<void> {
  let entries: import("node:fs").Dirent[];
  try {
    entries = await readdir(dir, { withFileTypes: true });
  } catch {
    return;
  }
  for (const e of entries) {
    const abs = join(dir, e.name);
    if (e.isDirectory()) {
      if (!SKIP_DIRS.has(e.name)) await walk(abs, out);
    } else if (EXTENSIONS.test(e.name)) {
      out.push(abs);
    }
  }
}

export async function runBanLint(root: string): Promise<{ findings: BanFinding[] }> {
  const files: string[] = [];
  for (const d of SCAN_DIRS) await walk(join(root, d), files);
  files.sort();
  const findings: BanFinding[] = [];
  for (const abs of files) {
    const path = relative(root, abs).replace(/[\\]/g, "/");
    if (LAB_EXEMPT.test(path)) continue;
    const text = await readFile(abs, "utf8");
    for (const hit of lintFile({ path, text })) {
      findings.push({ rule: hit.rule, path, line: hit.line, match: hit.match });
    }
  }
  return { findings };
}

async function main(): Promise<void> {
  const root = fileURLToPath(new URL("..", import.meta.url));
  const { findings } = await runBanLint(root);
  if (findings.length === 0) {
    console.log("0 findings");
    return;
  }
  for (const f of findings) console.log(`${f.path}:${f.line}  ${f.rule}  ${f.match}`);
  console.log(`${findings.length} findings`);
  process.exitCode = 1;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  await main();
}
