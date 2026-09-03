import { readFileSync } from "node:fs"
import type { ServerResponse } from "node:http"
import { extname, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import type { Plugin } from "vite"

import {
  brandAssetContentType,
  brandServedRelativePaths,
} from "./brandPublicAssets"

const brandAssetRoot = fileURLToPath(
  new URL("../../packages/ui/src/assets/brand/", import.meta.url),
)
const brandAssetManifestPath = resolve(brandAssetRoot, "manifest.json")
const webManifestPath = fileURLToPath(
  new URL("./site.webmanifest", import.meta.url),
)
const brandServedAssets = brandServedRelativePaths.map((relativePath) => ({
  publicName: relativePath.slice(relativePath.lastIndexOf("/") + 1),
  sourcePath: resolve(brandAssetRoot, relativePath),
}))

function brandMetadata(): Plugin {
  return {
    name: "chenchess-brand-metadata",
    buildStart() {
      this.addWatchFile(brandAssetManifestPath)
      this.addWatchFile(webManifestPath)
      for (const { sourcePath } of brandServedAssets) {
        this.addWatchFile(sourcePath)
      }
    },
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        const pathname = new URL(request.url ?? "/", "http://vite.invalid")
          .pathname
        if (pathname === "/site.webmanifest") {
          sendDevelopmentAsset(
            response,
            readFileSync(webManifestPath),
            "application/manifest+json; charset=utf-8",
          )
          return
        }
        const prefix = "/brand/"
        const publicName = pathname.startsWith(prefix)
          ? pathname.slice(prefix.length)
          : ""
        const asset = brandServedAssets.find(
          (candidate) => candidate.publicName === publicName,
        )
        if (!asset) {
          next()
          return
        }
        sendDevelopmentAsset(
          response,
          readFileSync(asset.sourcePath),
          brandAssetContentType(extname(publicName).toLowerCase()),
        )
      })
    },
    generateBundle() {
      this.emitFile({
        fileName: "site.webmanifest",
        source: readFileSync(webManifestPath),
        type: "asset",
      })
      for (const { publicName, sourcePath } of brandServedAssets) {
        this.emitFile({
          fileName: `brand/${publicName}`,
          source: readFileSync(sourcePath),
          type: "asset",
        })
      }
    },
  }
}

function sendDevelopmentAsset(
  response: ServerResponse,
  source: string | Buffer,
  contentType: string,
) {
  response.writeHead(200, {
    "Cache-Control": "no-cache",
    "Content-Length": Buffer.byteLength(source),
    "Content-Type": contentType,
  })
  response.end(source)
}

export function centralHostVitePlugins(): Plugin[] {
  return [brandMetadata()]
}

export const centralHostViteResolve = {
  alias: {
    "@": fileURLToPath(new URL("./src", import.meta.url)),
  },
  dedupe: ["react", "react-dom"],
}
