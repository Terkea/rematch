import type { MetadataRoute } from "next";
import { brand } from "../brand";

// The web app manifest (served at /manifest.webmanifest). The icons are the boy with his board on
// paper, made by `pnpm --filter @ci/web brand` into public/.
export default function manifest(): MetadataRoute.Manifest {
  return {
    name: `${brand.name}, chess improver`,
    short_name: brand.name,
    start_url: "/",
    display: "standalone",
    icons: [
      { src: "/icon-192.png", sizes: "192x192", type: "image/png" },
      { src: "/icon-512.png", sizes: "512x512", type: "image/png" },
    ],
  };
}
