import { fileURLToPath } from "node:url"
import { defineConfig } from "vitest/config"

import { chenStylexVitePlugin } from "./stylex.vite.mjs"

export default defineConfig({
  plugins: [chenStylexVitePlugin()],
  test: {
    alias: {
      // Vitest transforms modules without the StyleX compiler; the shim keeps
      // authored `stylex.create` imports inert. See vitest.stylexShim.ts.
      "@stylexjs/stylex": fileURLToPath(
        new URL("./src/vitest.stylexShim.ts", import.meta.url),
      ),
    },
    setupFiles: [
      fileURLToPath(new URL("./src/vitest.setup.ts", import.meta.url)),
    ],
  },
})
