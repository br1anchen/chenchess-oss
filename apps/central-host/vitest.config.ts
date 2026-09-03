import { chenStylexVitePlugin } from "@chenchess/ui/stylex.vite"
import react from "@vitejs/plugin-react"
import { fileURLToPath } from "node:url"
import { defineConfig } from "vitest/config"

export default defineConfig({
  plugins: [chenStylexVitePlugin(), react()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
    dedupe: ["react", "react-dom"],
  },
  test: {
    alias: {
      // Vitest transforms modules without the StyleX compiler; the shim keeps
      // authored `stylex.create` imports inert (see packages/ui).
      "@stylexjs/stylex": fileURLToPath(
        new URL("../../packages/ui/src/vitest.stylexShim.ts", import.meta.url),
      ),
    },
    setupFiles: [fileURLToPath(new URL("./vitest.setup.ts", import.meta.url))],
  },
})
