import { fileURLToPath } from "node:url";
import type { NextConfig } from "next";

// ADR 0026: workspace packages resolve source through the `development` export condition. Next 16
// exposes no custom export condition, and Turbopack cannot map the `./x.js` imports in @ci/db and
// @ci/chess-core to their `.ts` files (no extensionAlias). So both `next dev` and `next build` run on
// webpack (ADR-WEB-2). In development only, the two packages are aliased to their source entry points;
// production builds resolve them through the normal export map (dist, built first by turbo's ^build).
const dev = process.env.NODE_ENV !== "production";
const source = (path: string) => fileURLToPath(new URL(path, import.meta.url));

const nextConfig: NextConfig = {
  output: "standalone",
  outputFileTracingRoot: source("../.."),
  transpilePackages: ["@ci/ui", "@ci/db", "@ci/chess-core"],
  webpack(config) {
    config.resolve.extensionAlias = {
      ...config.resolve.extensionAlias,
      ".js": [".ts", ".tsx", ".js"],
    };
    if (dev) {
      config.resolve.alias = {
        ...config.resolve.alias,
        "@ci/db": source("../../packages/db/src/index.ts"),
        "@ci/chess-core": source("../../packages/chess-core/src/index.ts"),
      };
    }
    return config;
  },
};

export default nextConfig;
