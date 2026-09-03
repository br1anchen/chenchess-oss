import { resolve } from "node:path"

import { createCoachProtocolAdmission } from "./server/protocol-admission.js"
import { createWebOrigin } from "./server.js"

const coachEngineBaseUrl = process.env.COACH_ENGINE_BASE_URL
if (!coachEngineBaseUrl) {
  throw new Error("COACH_ENGINE_BASE_URL is required")
}
const port = Number(process.env.PORT ?? "3000")
if (!Number.isInteger(port) || port < 1 || port > 65_535) {
  throw new Error("PORT must be an integer from 1 to 65535")
}
const admission = createCoachProtocolAdmission()
const server = createWebOrigin({
  admission,
  coachEngineBaseUrl,
  staticRoot: process.env.WEB_STATIC_ROOT ?? resolve("dist"),
})
server.listen(port, "0.0.0.0", () => {
  process.stdout.write(`central host listening on ${port}\n`)
})

let shuttingDown = false
const shutdown = () => {
  if (shuttingDown) return
  shuttingDown = true
  server.close((error) => {
    process.exit(error ? 1 : 0)
  })
  setTimeout(() => {
    server.closeAllConnections()
    process.exit(1)
  }, 10_000).unref()
}
process.once("SIGINT", shutdown)
process.once("SIGTERM", shutdown)
