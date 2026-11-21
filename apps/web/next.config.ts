import { fileURLToPath } from "node:url";
import type { NextConfig } from "next";

// ADR 0026: workspace packages resolve source through the `development` export condition. Next 16
// exposes no custom export condition, and Turbopack cannot map the `./x.js` imports in @ci/db and
// @ci/chess-core to their `.ts` files (no extensionAlias). So `next dev` runs on webpack and, in
// development only, aliases both packages to their source entry points. Production builds
// (Turbopack) use dist, built first by turbo's ^build.
const dev = process.env.NODE_ENV !== "production";
const source = (path: string) => fileURLToPath(new URL(path, import.meta.url));

const nextConfig: NextConfig = {
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
