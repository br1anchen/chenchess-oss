import { chenStylexVitePlugin } from "@chenchess/ui/stylex.vite"
import react from "@vitejs/plugin-react"
import { fileURLToPath, URL } from "node:url"
import { defineConfig } from "vitest/config"

import { centralHostVitePlugins, centralHostViteResolve } from "./vite.surface"

/**
 * The Vitest config. `astro build` writes the production `dist/` and
 * `.storybook/vite.config.ts` serves Storybook, so nothing here starts a dev
 * server; it exists to give the test graph the same plugins and aliases the
 * app is built with.
 */
export default defineConfig({
  // The web surface always uses the disabled facade. Removing every complete
  // labeled telemetry statement also lets Rollup discard that facade and its
  // instrumentation-only imports rather than shipping callable no-ops.
  esbuild: { dropLabels: ["TELEMETRY"] },
  plugins: [...centralHostVitePlugins(), chenStylexVitePlugin(), react()],
  resolve: centralHostViteResolve,
  test: {
    setupFiles: [fileURLToPath(new URL("./vitest.setup.ts", import.meta.url))],
  },
})
