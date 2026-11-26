// Bundles the lab's Node step (scripts/lab-bridge.ts, ADR 0115) into one ES module that runs on
// plain Node with no node_modules: `node scripts/build-lab-bridge.mjs <outfile>`. The lab image
// runs it in its build stage; nothing in the product uses the bundle. The bundle is built from
// the same files the app imports (the `development` condition reads @ci/chess-core's sources).
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const outfile = process.argv[2];
if (!outfile) {
  console.error("usage: node scripts/build-lab-bridge.mjs <outfile>");
  process.exit(1);
}
await build({
  entryPoints: [fileURLToPath(new URL("./lab-bridge.ts", import.meta.url))],
  bundle: true,
  platform: "node",
  format: "esm",
  target: "node22",
  conditions: ["development"],
  outfile,
  legalComments: "none",
  logLevel: "warning",
});
console.log(`wrote ${outfile}`);
