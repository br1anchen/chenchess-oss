import type { Server } from "node:http"
import * as v from "valibot"

/** Binds a test server to an ephemeral loopback port and reports its origin. */
export async function listen(server: Server, port = 0): Promise<string> {
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject)
    server.listen(port, "127.0.0.1", () => {
      server.off("error", reject)
      resolve()
    })
  })
  const address = server.address()
  if (!address || parseIsString(address)) {
    throw new Error("Test server did not bind a TCP port")
  }
  return `http://127.0.0.1:${address.port}`
}

export async function close(server: Server) {
  if (!server.listening) return
  await new Promise<void>((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()))
  })
}

/**
 * The server closes the socket on a rejected request, so a pooled connection
 * can still be draining when the next attempt reuses it. Retry briefly before
 * surfacing the transport error, otherwise the assertion never runs.
 */
export async function fetchAfterConnectionReset(
  input: string,
  init: RequestInit,
) {
  let lastError: unknown
  for (let attempt = 0; attempt < 5; attempt += 1) {
    try {
      return await fetch(input, init)
    } catch (error) {
      lastError = error
      await new Promise((resolve) => setTimeout(resolve, 20))
    }
  }
  throw lastError
}

function parseIsString(value: unknown): value is string {
  return v.is(v.string(), value)
}
