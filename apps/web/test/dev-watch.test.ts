import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import nextConfig from "../next.config";

// M7: a browser-automation tool wrote `.playwright-mcp/` into the repo, which git offered to
// commit and the dev server watched, so every file it wrote set off Fast Refresh.

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "../../..");

describe("tool output stays out of git and the dev watcher (M7)", () => {
  it("git ignores .playwright-mcp/", () => {
    const lines = readFileSync(join(ROOT, ".gitignore"), "utf8").split(/\r?\n/);
    expect(lines).toContain(".playwright-mcp/");
  });

  it("webpack's watcher ignores it, keeping node_modules and .git ignored", () => {
    const webpack = nextConfig.webpack;
    if (!webpack) throw new Error("no webpack hook");
    const out = webpack(
      { resolve: {}, watchOptions: { ignored: ["**/node_modules/**", "**/.git/**"] } },
      {} as never,
    ) as { watchOptions: { ignored: string[] } };
    expect(out.watchOptions.ignored).toEqual(
      expect.arrayContaining(["**/node_modules/**", "**/.git/**", "**/.playwright-mcp/**"]),
    );
  });

  it("extends a RegExp watcher rule the same way", () => {
    const webpack = nextConfig.webpack;
    if (!webpack) throw new Error("no webpack hook");
    const out = webpack(
      { resolve: {}, watchOptions: { ignored: /[\\/]node_modules[\\/]/ } },
      {} as never,
    ) as { watchOptions: { ignored: RegExp } };
    const rule = out.watchOptions.ignored;
    expect(rule.test("/app/node_modules/x.js")).toBe(true);
    expect(rule.test("/app/.playwright-mcp/page.png")).toBe(true);
    expect(rule.test("E:\\code\\.playwright-mcp\\page.png")).toBe(true);
    expect(rule.test("/app/src/playwright-mcp.ts")).toBe(false);
  });
});
