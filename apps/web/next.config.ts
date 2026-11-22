import { fileURLToPath } from "node:url";
import type { NextConfig } from "next";

// ADR 0026: workspace packages resolve source through the `development` export condition. Next 16
// exposes no custom export condition, and Turbopack cannot map the `./x.js` imports in @ci/db and
// @ci/chess-core to their `.ts` files (no extensionAlias). So both `next dev` and `next build` run on
// webpack (ADR-WEB-2). In development only, the two packages' entry points are aliased to source;
// production builds resolve them through the normal export map (dist, built first by turbo's ^build).
const dev = process.env.NODE_ENV !== "production";
const source = (path: string) => fileURLToPath(new URL(path, import.meta.url));

const nextConfig: NextConfig = {
  output: "standalone",
  // Keeps the dev indicator out of styleguide screenshots.
  devIndicators: false,
  // `next dev` would otherwise write an AGENTS.md when it detects a coding agent (T7 ruling, T24);
  // the project skills and CLAUDE.md are the agent rules here.
  agentRules: false,
  // The Compose `qa` service loads the dev server as http://web:3100; Next 16 blocks dev assets
  // (and so hydration) for origins not listed here.
  allowedDevOrigins: ["web"],
  outputFileTracingRoot: source("../.."),
  transpilePackages: ["@ci/ui", "@ci/db", "@ci/chess-core"],
  experimental: {
    // `next dev` restarts itself past 80% of its heap. In a long e2e run that drops connections
    // and reloads pages mid-spec (M5b T11), so docker-compose.e2e.yml turns it off with
    // NEXT_DEV_MEMORY_RESTART=0 and gives the server a larger heap instead.
    devMemoryThresholdRestart: process.env.NEXT_DEV_MEMORY_RESTART !== "0",
  },
  webpack(config) {
    config.resolve.extensionAlias = {
      ...config.resolve.extensionAlias,
      ".js": [".ts", ".tsx", ".js"],
    };
    if (dev) {
      config.resolve.alias = {
        ...config.resolve.alias,
        // Exact matches (`$`): a bare "@ci/db" alias would also rewrite "@ci/db/generated" to
        // ".../index.ts/generated", which breaks /api/events in development (T27).
        "@ci/db$": source("../../packages/db/src/index.ts"),
        "@ci/db/generated$": source("../../packages/db/src/generated/schema.ts"),
        "@ci/db/generated/relations$": source("../../packages/db/src/generated/relations.ts"),
        "@ci/chess-core$": source("../../packages/chess-core/src/index.ts"),
      };
    }
    return config;
  },
};

export default nextConfig;
