import { mkdir, mkdtemp, writeFile } from "node:fs/promises"
import { createServer, type Server } from "node:http"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { afterEach, beforeEach, describe, expect, test } from "vitest"

import { createWebOrigin } from "./server"
import * as v from "valibot"

let upstream: Server
let origin: Server
let upstreamUrl: string
let originUrl: string
let staticRoot: string
let upstreamRequestUrls: string[]
let upstreamRequestBodies: string[]
let upstreamSourceIps: Array<string | undefined>

beforeEach(async () => {
  upstreamRequestUrls = []
  upstreamRequestBodies = []
  upstreamSourceIps = []
  staticRoot = await mkdtemp(join(tmpdir(), "chenchess-web-origin-"))
  await mkdir(join(staticRoot, "app"))
  await mkdir(join(staticRoot, "backoffice"))
  await mkdir(join(staticRoot, "dashboard"))
  await mkdir(join(staticRoot, "join"))
  await mkdir(join(staticRoot, "login"))
  await mkdir(join(staticRoot, "privacy"))
  await mkdir(join(staticRoot, "preview"))
  await mkdir(join(staticRoot, "support"))
  await mkdir(join(staticRoot, "terms"))
  await writeFile(join(staticRoot, "index.html"), "<h1>Landing Page</h1>")
  await writeFile(join(staticRoot, "app/index.html"), "<h1>Coach App</h1>")
  await writeFile(
    join(staticRoot, "backoffice/index.html"),
    "<h1>Beta Back Office</h1>",
  )
  await writeFile(join(staticRoot, "join/index.html"), "<h1>Join</h1>")
  await writeFile(
    join(staticRoot, "dashboard/index.html"),
    "<h1>Player Dashboard</h1>",
  )
  await writeFile(join(staticRoot, "login/index.html"), "<h1>Login</h1>")
  await writeFile(join(staticRoot, "privacy/index.html"), "<h1>Privacy</h1>")
  await writeFile(
    join(staticRoot, "preview/index.html"),
    "<h1>Preview Catalog</h1>",
  )
  await writeFile(join(staticRoot, "support/index.html"), "<h1>Support</h1>")
  await writeFile(join(staticRoot, "terms/index.html"), "<h1>Terms</h1>")
  await writeFile(join(staticRoot, "app.js"), "window.chenChess = true")

  upstream = createServer((request, response) => {
    upstreamRequestUrls.push(request.url ?? "")
    upstreamSourceIps.push(request.headers["x-chenchess-source-ip"])
    const chunks: Buffer[] = []
    request.on("data", (chunk: Buffer) => chunks.push(chunk))
    request.on("end", () => {
      const body = Buffer.concat(chunks).toString()
      upstreamRequestBodies.push(body)
      if (request.url === "/api/v1/beta-access/requests") {
        const payload = JSON.stringify({
          message: "Thanks. Your beta access request has been received.",
        })
        response.writeHead(202, {
          "Content-Length": Buffer.byteLength(payload),
          "Content-Type": "application/json",
        })
        response.end(payload)
        return
      }
      if (request.url === "/api/v1/beta-access/authorization") {
        const authorization = request.headers.authorization ?? ""
        const admitted = authorization === "Bearer admitted-player-token"
        const payload = JSON.stringify(
          admitted
            ? { playerId: "firebase-player-a" }
            : { error: "Beta Access is required" },
        )
        response.writeHead(admitted ? 200 : 403, {
          "Cache-Control": "no-store",
          "Content-Length": Buffer.byteLength(payload),
          "Content-Type": "application/json",
          "X-Upstream-Authorization": authorization,
        })
        response.end(payload)
        return
      }
      if (request.url?.startsWith("/__/auth/")) {
        response.writeHead(200, {
          "Content-Type": "text/html; charset=utf-8",
          "X-Firebase-Auth-Helper": "proxied",
        })
        response.end(`<p>${body || "firebase-auth-helper"}</p>`)
        return
      }
      response.writeHead(207, {
        "Content-Type": "application/x-ndjson",
        "X-Upstream-Authorization": request.headers.authorization ?? "",
        Connection: "close",
      })
      response.write(body)
      response.end('\n{"event":"complete"}\n')
    })
  })
  upstreamUrl = await listen(upstream)
  origin = createWebOrigin({
    coachEngineBaseUrl: upstreamUrl,
    firebaseAuthHelperOrigin: upstreamUrl,
    staticRoot,
  })
  originUrl = await listen(origin)
})

afterEach(async () => {
  await Promise.all([close(origin), close(upstream)])
})

describe("thin Node web origin", () => {
  test("serves health, static assets, and the SPA fallback", async () => {
    const health = await fetch(`${originUrl}/health`)
    const healthDocument = await health.json()
    expect(healthDocument).toEqual({
      bootId: expect.stringMatching(
        /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
      ),
      ok: true,
      service: "central-host",
    })
    expect(await (await fetch(`${originUrl}/health`)).json()).toEqual(
      healthDocument,
    )

    const asset = await fetch(`${originUrl}/app.js`)
    expect(asset.headers.get("content-type")).toContain("text/javascript")
    expect(await asset.text()).toBe("window.chenChess = true")

    const route = await fetch(
      `${originUrl}/app/game-reviews/game-import%3Afixture%3Across-surface/moments/review-moment%3Afixture%3Aone`,
    )
    expectStaticSecurityHeaders(route)
    expect(await route.text()).toBe("<h1>Coach App</h1>")
    const root = await fetch(`${originUrl}/`)
    expectStaticSecurityHeaders(root)
    expect(await root.text()).toBe("<h1>Landing Page</h1>")
    expect((await fetch(`${originUrl}/missing.js`)).status).toBe(404)
  })

  test("keeps public and authenticated routes distinct", async () => {
    for (const [pathname, heading] of [
      ["/join", "Join"],
      ["/join/invitation", "Join"],
      ["/login", "Login"],
      ["/login/verification", "Login"],
      ["/privacy/", "Privacy"],
      ["/support", "Support"],
      ["/terms/", "Terms"],
    ]) {
      const response = await fetch(`${originUrl}${pathname}`)
      expectStaticSecurityHeaders(response)
      expect(await response.text()).toBe(`<h1>${heading}</h1>`)
    }

    for (const pathname of ["/not-a-surface", "/privacy/not-a-page"]) {
      const response = await fetch(`${originUrl}${pathname}`)
      expect({ pathname, status: response.status }).toEqual({
        pathname,
        status: 404,
      })
      expect(await response.text()).toBe("Not found\n")
    }

    // The OAuth authorization server and the remote MCP endpoint are not part
    // of this snapshot, so their addresses are ordinary misses.
    for (const pathname of ["/auth", "/mcp", "/interaction/example"]) {
      expect({
        pathname,
        status: (await fetch(`${originUrl}${pathname}`)).status,
      }).toEqual({
        pathname,
        status: 404,
      })
    }
  })

  test("serves the branded 404 page when dist has one", async () => {
    await writeFile(join(staticRoot, "404.html"), "<h1>Page not found</h1>")
    const response = await fetch(`${originUrl}/not-a-surface`)
    expect(response.status).toBe(404)
    expect(await response.text()).toBe("<h1>Page not found</h1>")
  })

  test("proxies only the reserved Firebase Authentication helper namespace", async () => {
    const iframe = await fetch(
      `${originUrl}/__/auth/iframe?apiKey=public-web-config`,
    )
    expect(iframe.status).toBe(200)
    expect(iframe.headers.get("x-firebase-auth-helper")).toBe("proxied")
    expect(iframe.headers.get("x-robots-tag")).toBe("noindex, nofollow")
    expect(await iframe.text()).toBe("<p>firebase-auth-helper</p>")

    const handler = await fetch(`${originUrl}/__/auth/handler`, {
      body: "oauth=callback",
      method: "POST",
    })
    expect(handler.status).toBe(200)
    expect(await handler.text()).toBe("<p>oauth=callback</p>")

    const unsupported = await fetch(`${originUrl}/__/auth/handler`, {
      method: "DELETE",
    })
    expect(unsupported.status).toBe(405)
    expect(unsupported.headers.get("allow")).toBe("GET, HEAD, POST")

    expect((await fetch(`${originUrl}/__/auth`)).status).toBe(404)
    expect((await fetch(`${originUrl}/__/firebase/init.json`)).status).toBe(404)
    expect(upstreamRequestUrls).toEqual([
      "/__/auth/iframe?apiKey=public-web-config",
      "/__/auth/handler",
    ])
  })

  test("forwards authenticated command and event bytes without interpretation", async () => {
    const command = '{"kind":"opaque-command","raw":"陳"}'
    const response = await fetch(
      `${originUrl}/api/v1/review-session/commands?stream=true`,
      {
        body: command,
        headers: {
          Authorization: "Bearer firebase-id-token",
          "Content-Type": "application/json",
        },
        method: "POST",
      },
    )

    expect(response.status).toBe(207)
    expect(response.headers.get("x-upstream-authorization")).toBe(
      "Bearer firebase-id-token",
    )
    expect(response.headers.get("connection")).not.toBe("close")
    expect(await response.text()).toBe(`${command}\n{"event":"complete"}\n`)
    expect(upstreamRequestUrls).toEqual([
      "/api/v1/review-session/commands?stream=true",
    ])
    expect(upstreamSourceIps).toEqual([undefined])
  })

  test("relays Coach Engine Beta Access admission and revocation without deciding it", async () => {
    for (const [token, status] of [
      ["uninvited-player-token", 403],
      ["admitted-player-token", 200],
      ["revoked-player-token", 403],
    ] as const) {
      const response = await fetch(
        `${originUrl}/api/v1/beta-access/authorization`,
        { headers: { Authorization: `Bearer ${token}` } },
      )
      expect(response.status).toBe(status)
      expect(response.headers.get("x-upstream-authorization")).toBe(
        `Bearer ${token}`,
      )
      expect(response.headers.get("cache-control")).toBe("no-store")
    }
    expect(upstreamRequestUrls).toEqual([
      "/api/v1/beta-access/authorization",
      "/api/v1/beta-access/authorization",
      "/api/v1/beta-access/authorization",
    ])
    expect(upstreamSourceIps).toEqual([undefined, undefined, undefined])
  })

  test("relays a validated Railway source IP only for beta access endpoints", async () => {
    const response = await fetch(`${originUrl}/api/v1/beta-access/requests`, {
      headers: {
        Authorization: "Bearer verified-firebase-token",
        "X-ChenChess-Source-Ip": "198.51.100.99",
        "X-Real-IP": "203.0.113.7",
      },
      method: "POST",
    })

    expect(response.status).toBe(202)
    expect(await response.json()).toEqual({
      message: "Thanks. Your beta access request has been received.",
    })
    expect(upstreamRequestUrls).toEqual(["/api/v1/beta-access/requests"])
    expect(upstreamRequestBodies).toEqual([""])
    expect(upstreamSourceIps).toEqual(["203.0.113.7"])

    await fetch(`${originUrl}/api/v1/beta-access/requests`, {
      headers: {
        Authorization: "Bearer verified-firebase-token",
        "X-Real-IP": "invalid",
      },
      method: "POST",
    })
    expect(upstreamSourceIps).toEqual(["203.0.113.7", undefined])

    await fetch(`${originUrl}/api/v1/beta-access/invitations/redeem`, {
      body: '{"code":"private-invitation-code"}',
      headers: {
        Authorization: "Bearer firebase-id-token",
        "Content-Type": "application/json",
        "X-Real-IP": "203.0.113.8",
      },
      method: "POST",
    })
    await fetch(`${originUrl}/api/v1/review-session/commands`, {
      body: "{}",
      headers: { "X-Real-IP": "203.0.113.9" },
      method: "POST",
    })
    expect(upstreamSourceIps).toEqual([
      "203.0.113.7",
      undefined,
      "203.0.113.8",
      undefined,
    ])
  })

  test("rejects a Beta Access Request body without forwarding it", async () => {
    const response = await fetch(`${originUrl}/api/v1/beta-access/requests`, {
      body: '{"email":"other@example.test"}',
      headers: {
        Authorization: "Bearer verified-firebase-token",
        "Content-Type": "application/json",
      },
      method: "POST",
    })

    expect(response.status).toBe(413)
    expect(await response.text()).toBe("Request body not allowed\n")
    expect(upstreamRequestUrls).toEqual([])
  })

  test("keeps health independent when Coach Engine is unavailable", async () => {
    await close(upstream)
    expect((await fetch(`${originUrl}/health`)).status).toBe(200)
    expect(
      (
        await fetch(`${originUrl}/api/v1/review-session/commands`, {
          body: "{}",
          method: "POST",
        })
      ).status,
    ).toBe(502)
  })
})

async function listen(server: Server): Promise<string> {
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject)
    server.listen(0, "127.0.0.1", () => {
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

async function close(server: Server) {
  if (!server.listening) return
  await new Promise<void>((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()))
  })
}

function expectStaticSecurityHeaders(response: Response) {
  expect(response.headers.get("content-security-policy")).toBe(
    "base-uri 'none'; frame-ancestors 'none'; object-src 'none'",
  )
  expect(response.headers.get("cross-origin-opener-policy")).toBe(
    "same-origin-allow-popups",
  )
  expect(response.headers.get("permissions-policy")).toBe(
    "camera=(), geolocation=(), microphone=()",
  )
  expect(response.headers.get("referrer-policy")).toBe("no-referrer")
  expect(response.headers.get("x-content-type-options")).toBe("nosniff")
  expect(response.headers.get("x-frame-options")).toBe("DENY")
  expect(response.headers.get("x-robots-tag")).toBe("noindex, nofollow")
}

function parseIsString(value: unknown): value is string {
  return v.is(v.string(), value)
}
