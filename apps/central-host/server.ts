import { randomUUID } from "node:crypto"
import { createReadStream } from "node:fs"
import { stat } from "node:fs/promises"
import { createServer, request as httpRequest } from "node:http"
import { request as httpsRequest } from "node:https"
import { isIP } from "node:net"
import { extname, resolve, sep } from "node:path"

import type {
  IncomingHttpHeaders,
  IncomingMessage,
  RequestListener,
  Server,
  ServerResponse,
} from "node:http"

import { spaSurfaceRootFor } from "./src/siteSurfaces.js"
import {
  admitHealthRequest,
  createCoachProtocolAdmission,
  type CoachProtocolAdmission,
} from "./server/protocol-admission.js"
import * as v from "valibot"

export type WebOriginOptions = {
  admission?: CoachProtocolAdmission
  coachEngineBaseUrl: string
  firebaseAuthHelperOrigin?: string
  staticRoot: string
  webHandler?: RequestListener
}

const hopByHopHeaders = new Set([
  "connection",
  "keep-alive",
  "proxy-authenticate",
  "proxy-authorization",
  "te",
  "trailer",
  "transfer-encoding",
  "upgrade",
])
const betaAccessSourceIpPaths = new Set([
  "/api/v1/beta-access/requests",
  "/api/v1/beta-access/invitations/redeem",
])
const emptyBetaAccessRequestPath = "/api/v1/beta-access/requests"
const sourceIpHeader = "x-chenchess-source-ip"
const centralHostBootId = randomUUID()

function contentTypeFor(extension: string) {
  switch (extension) {
    case ".css":
      return contentTypes[".css"]
    case ".html":
      return contentTypes[".html"]
    case ".ico":
      return contentTypes[".ico"]
    case ".jpg":
      return contentTypes[".jpg"]
    case ".js":
      return contentTypes[".js"]
    case ".json":
      return contentTypes[".json"]
    case ".png":
      return contentTypes[".png"]
    case ".svg":
      return contentTypes[".svg"]
    case ".webmanifest":
      return contentTypes[".webmanifest"]
    case ".webp":
      return contentTypes[".webp"]
    default:
      return "application/octet-stream"
  }
}

const contentTypes = {
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".ico": "image/x-icon",
  ".jpg": "image/jpeg",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".webmanifest": "application/manifest+json; charset=utf-8",
  ".webp": "image/webp",
}

export function createWebOrigin({
  admission = createCoachProtocolAdmission(),
  coachEngineBaseUrl,
  firebaseAuthHelperOrigin,
  staticRoot,
  webHandler,
}: WebOriginOptions): Server {
  const upstream = new URL(coachEngineBaseUrl)
  if (upstream.protocol !== "http:" && upstream.protocol !== "https:") {
    throw new Error("COACH_ENGINE_BASE_URL must use HTTP or HTTPS")
  }
  const firebaseAuthUpstream = firebaseAuthHelperOrigin
    ? new URL(firebaseAuthHelperOrigin)
    : null
  const root = resolve(staticRoot)

  return createServer((request, response) => {
    void routeRequest(
      request,
      response,
      root,
      upstream,
      firebaseAuthUpstream,
      admission,
      webHandler,
    )
  })
}

async function routeRequest(
  request: IncomingMessage,
  response: ServerResponse,
  staticRoot: string,
  upstream: URL,
  firebaseAuthUpstream: URL | null,
  admission: CoachProtocolAdmission,
  webHandler?: RequestListener,
) {
  const url = new URL(request.url ?? "/", "http://web.invalid")
  if (url.pathname === "/health") {
    sendHealth(request, response, admission)
    return
  }
  if (url.pathname === "/api" || url.pathname.startsWith("/api/")) {
    proxyApi(request, response, url, upstream)
    return
  }
  if (firebaseAuthUpstream && url.pathname.startsWith("/__/auth/")) {
    proxyFirebaseAuthHelper(request, response, url, firebaseAuthUpstream)
    return
  }
  if (webHandler) {
    webHandler(request, response)
    return
  }
  await serveWebAsset(request, response, url, staticRoot)
}

function sendHealth(
  request: IncomingMessage,
  response: ServerResponse,
  admission: CoachProtocolAdmission,
) {
  if (!admitHealthRequest(admission, request, response)) return
  if (request.method !== "GET" && request.method !== "HEAD") {
    sendText(response, 405, "Method not allowed\n", {
      Allow: "GET, HEAD",
    })
    return
  }
  const body = `${JSON.stringify({
    bootId: centralHostBootId,
    ok: true,
    service: "central-host",
  })}\n`
  response.writeHead(200, {
    "Cache-Control": "no-store",
    "Content-Length": Buffer.byteLength(body),
    "Content-Type": "application/json; charset=utf-8",
  })
  response.end(request.method === "HEAD" ? undefined : body)
}

function proxyApi(
  request: IncomingMessage,
  response: ServerResponse,
  url: URL,
  upstream: URL,
) {
  const emptyRequest =
    url.pathname === emptyBetaAccessRequestPath && request.method === "POST"
  if (emptyRequest && hasDeclaredRequestBody(request.headers)) {
    request.resume()
    sendText(response, 413, "Request body not allowed\n")
    return
  }
  const destination = new URL(`${url.pathname}${url.search}`, upstream)
  const headers = forwardedHeaders(request.headers)
  if (betaAccessSourceIpPaths.has(url.pathname)) {
    const sourceIp = trustedSourceIp(request)
    if (sourceIp) headers[sourceIpHeader] = sourceIp
  }
  proxyRequest(
    request,
    response,
    destination,
    headers,
    "Coach Engine unavailable\n",
    {
      forwardRequestBody: !emptyRequest,
      responseHeaders: { "cache-control": "no-store" },
    },
  )
}

function proxyFirebaseAuthHelper(
  request: IncomingMessage,
  response: ServerResponse,
  url: URL,
  upstream: URL,
) {
  if (
    request.method !== "GET" &&
    request.method !== "HEAD" &&
    request.method !== "POST"
  ) {
    sendText(response, 405, "Method not allowed\n", {
      Allow: "GET, HEAD, POST",
    })
    return
  }
  const destination = new URL(`${url.pathname}${url.search}`, upstream)
  proxyRequest(
    request,
    response,
    destination,
    forwardedHeaders(request.headers),
    "Firebase Authentication unavailable\n",
    {
      responseHeaders: { "x-robots-tag": "noindex, nofollow" },
    },
  )
}

type ProxyRequestOptions = {
  forwardRequestBody?: boolean
  responseHeaders?: Record<string, string>
}

function proxyRequest(
  request: IncomingMessage,
  response: ServerResponse,
  destination: URL,
  headers: ReturnType<typeof forwardedHeaders>,
  unavailableBody: string,
  { forwardRequestBody = true, responseHeaders = {} }: ProxyRequestOptions = {},
) {
  const send = destination.protocol === "https:" ? httpsRequest : httpRequest
  const upstreamRequest = send(
    destination,
    {
      headers,
      method: request.method,
    },
    (upstreamResponse) => {
      response.writeHead(upstreamResponse.statusCode ?? 502, {
        ...forwardedHeaders(upstreamResponse.headers),
        ...responseHeaders,
      })
      upstreamResponse.pipe(response)
    },
  )

  upstreamRequest.on("error", () => {
    if (!response.headersSent) {
      sendText(response, 502, unavailableBody)
    } else {
      response.destroy()
    }
  })
  response.on("close", () => {
    if (!response.writableEnded) upstreamRequest.destroy()
  })
  if (forwardRequestBody) {
    request.pipe(upstreamRequest)
  } else {
    request.resume()
    upstreamRequest.end()
  }
}

async function serveWebAsset(
  request: IncomingMessage,
  response: ServerResponse,
  url: URL,
  staticRoot: string,
) {
  if (request.method !== "GET" && request.method !== "HEAD") {
    sendText(response, 405, "Method not allowed\n", {
      Allow: "GET, HEAD",
    })
    return
  }

  const requestedPath = safeStaticPath(staticRoot, url.pathname)
  if (!requestedPath) {
    sendText(response, 400, "Invalid path\n")
    return
  }
  const asset = await existingFile(requestedPath)
  const entry = surfaceEntry(staticRoot, url.pathname)
  const fallback = asset ?? (entry === null ? null : await existingFile(entry))
  if (!fallback) {
    const branded = await existingFile(resolve(staticRoot, "404.html"))
    if (!branded) {
      sendText(response, 404, "Not found\n")
      return
    }
    await sendStaticPage(request, response, branded, 404)
    return
  }

  await sendStaticPage(request, response, fallback, 200)
}

async function sendStaticPage(
  request: IncomingMessage,
  response: ServerResponse,
  filePath: string,
  status: 200 | 404,
) {
  const metadata = await stat(filePath)
  response.writeHead(status, {
    "Cache-Control": filePath.includes(`${sep}assets${sep}`)
      ? "public, max-age=31536000, immutable"
      : "no-cache",
    "Content-Length": metadata.size,
    "Content-Type": contentTypeFor(extname(filePath).toLowerCase()),
    "Content-Security-Policy":
      "base-uri 'none'; frame-ancestors 'none'; object-src 'none'",
    "Cross-Origin-Opener-Policy": "same-origin-allow-popups",
    "Permissions-Policy": "camera=(), geolocation=(), microphone=()",
    "Referrer-Policy": "no-referrer",
    "X-Content-Type-Options": "nosniff",
    "X-Frame-Options": "DENY",
    "X-Robots-Tag": "noindex, nofollow",
  })
  if (request.method === "HEAD") {
    response.end()
    return
  }
  createReadStream(filePath).pipe(response)
}

function surfaceEntry(staticRoot: string, pathname: string) {
  const root = spaSurfaceRootFor(pathname)
  if (root) return resolve(staticRoot, `${root.slice(1)}/index.html`)
  return pathname === "/" ? resolve(staticRoot, "index.html") : null
}

function safeStaticPath(staticRoot: string, pathname: string): string | null {
  let decoded: string
  try {
    decoded = decodeURIComponent(pathname)
  } catch {
    return null
  }
  const candidate = resolve(staticRoot, `.${decoded}`)
  return candidate === staticRoot || candidate.startsWith(`${staticRoot}${sep}`)
    ? candidate
    : null
}

async function existingFile(path: string): Promise<string | null> {
  try {
    const metadata = await stat(path)
    if (metadata.isFile()) return path
    if (metadata.isDirectory()) {
      const index = resolve(path, "index.html")
      return (await stat(index)).isFile() ? index : null
    }
  } catch {
    return null
  }
  return null
}

function forwardedHeaders(headers: IncomingHttpHeaders) {
  return Object.fromEntries(
    Object.entries(headers).filter(
      ([name, value]) =>
        value !== undefined &&
        name !== "host" &&
        name.toLowerCase() !== sourceIpHeader &&
        !hopByHopHeaders.has(name.toLowerCase()),
    ),
  )
}

function hasDeclaredRequestBody(headers: IncomingHttpHeaders) {
  return (
    headers["transfer-encoding"] !== undefined ||
    (headers["content-length"] !== undefined &&
      headers["content-length"] !== "0")
  )
}

function trustedSourceIp(request: IncomingMessage) {
  const railwaySourceIp = request.headers["x-real-ip"]
  if (railwaySourceIp !== undefined) {
    return parseIsString(railwaySourceIp) && isIP(railwaySourceIp) !== 0
      ? railwaySourceIp
      : null
  }
  const socketSourceIp = request.socket.remoteAddress
  return socketSourceIp && isIP(socketSourceIp) !== 0 ? socketSourceIp : null
}

function sendText(
  response: ServerResponse,
  status: number,
  body: string,
  headers: Record<string, string> = {},
) {
  response.writeHead(status, {
    "Content-Length": Buffer.byteLength(body),
    "Content-Type": "text/plain; charset=utf-8",
    ...headers,
  })
  response.end(body)
}

function parseIsString(value: unknown): value is string {
  return v.is(v.string(), value)
}
