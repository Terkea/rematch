import { readFileSync } from "node:fs";
import { runLabStep } from "../src/server/lab/bridge";

// The lab's Node step (ADR 0115): `lab-bridge <focus-target|focus-progress|trainer-sources>
// < request.json > response.json`. Run from source with `tsx --conditions development`, or as
// the bundle the lab image builds (`scripts/build-lab-bridge.mjs`). Exit codes: 0 done, 1 usage
// or a bad request (the reason on stderr).

try {
  process.stdout.write(runLabStep(process.argv[2] ?? "", readFileSync(0, "utf8")));
} catch (e) {
  process.stderr.write(`lab-bridge ${process.argv[2] ?? ""}: ${(e as Error).message}\n`);
  process.exitCode = 1;
}
