import react from "@astrojs/react"
import { chenStylexVitePlugin } from "@chenchess/ui/stylex.vite"
import { defineConfig } from "astro/config"

import { centralHostVitePlugins, centralHostViteResolve } from "./vite.surface"

/**
 * Astro writes `dist/` in static output. `server.ts` keeps the request path
 * (`/api`, `/health`, `/mcp`, OAuth, the auth proxy) and serves whatever
 * landed in `dist/`. No `/web/*` URL move.
 *
 * Asset URLs stay root-relative (`/assets/…`). The public origin belongs on
 * canonical / Open Graph tags, not Vite `base` — a full-origin base produced
 * protocol-relative `//assets` script URLs.
 */
export default defineConfig({
  output: "static",
  trailingSlash: "always",
  outDir: "dist",
  srcDir: "src",
  base: "/",
  build: {
    format: "directory",
    assets: "assets",
  },
  integrations: [react()],
  vite: {
    // Astro narrows Vite's client env exposure to `PUBLIC_*`. The auth entries
    // read `VITE_FIREBASE_*` from `import.meta.env`, so the Vite prefix has to
    // be named back in or every entry sees an empty Firebase config.
    envPrefix: ["VITE_", "PUBLIC_"],
    esbuild: { dropLabels: ["TELEMETRY"] },
    plugins: [...centralHostVitePlugins(), chenStylexVitePlugin()],
    resolve: centralHostViteResolve,
    server: {
      proxy: {
        "/api": "http://127.0.0.1:8787",
        "/health": "http://127.0.0.1:8787",
      },
    },
    ssr: { noExternal: [/@chenchess\//, /@astryxdesign\//] },
  },
})
