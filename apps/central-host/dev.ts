import { resolve } from "node:path"
import type { RequestListener } from "node:http"

import { getViteConfig } from "astro/config"
import { createServer as createViteServer } from "vite"

import { createCoachProtocolAdmission } from "./server/protocol-admission.js"
import { createWebOrigin } from "./server.js"
import { surfaceRouteUrl } from "./src/siteSurfaces.js"

const port = Number(process.env.PORT ?? "5173")
const coachEngineBaseUrl =
  process.env.COACH_ENGINE_BASE_URL ?? "http://127.0.0.1:8787"
const admission = createCoachProtocolAdmission()
let viteHandler: RequestListener = (_request, response) => {
  response.writeHead(503).end("Vite is starting\n")
}
const server = createWebOrigin({
  admission,
  coachEngineBaseUrl,
  staticRoot: resolve("dist"),
  webHandler: (request, response) => viteHandler(request, response),
})
const astroVite = await getViteConfig(
  {
    appType: "custom",
    server: { hmr: { server }, middlewareMode: true },
  },
  { root: resolve(import.meta.dirname) },
)({ command: "serve", mode: "development" })
const vite = await createViteServer(astroVite)
// Astro installs its request handler ahead of any plugin middleware, so the
// SPA deep-link rewrite has to sit in front of the whole Vite stack.
viteHandler = (request, response) => {
  const rewritten = request.url
    ? surfaceRouteUrl(new URL(request.url, "http://vite.invalid"))
    : undefined
  if (rewritten) request.url = rewritten
  vite.middlewares(request, response)
}

server.listen(port, "127.0.0.1", () => {
  process.stdout.write(
    `central host development origin listening at http://127.0.0.1:${port}\n`,
  )
})

let shuttingDown = false
const shutdown = () => {
  if (shuttingDown) return
  shuttingDown = true
  server.close((error) => {
    void vite.close().finally(() => process.exit(error ? 1 : 0))
  })
}
process.once("SIGINT", shutdown)
process.once("SIGTERM", shutdown)
