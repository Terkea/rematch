// The product version (ADR 0095). The root package.json is the source; release-please keeps this
// package's version in step with it (scripts/test-versions.mjs checks), and the build inlines it.
import pkg from "../package.json";

export const appVersion: string = pkg.version;

/** "Rematch v0.1.0 · beta": every 0.y.z is the beta; 1.0.0, the MVP, drops the word. */
export function versionLabel(name: string, version: string): string {
  const beta = Number(version.split(".")[0]) < 1;
  return `${name} v${version}${beta ? " · beta" : ""}`;
}
