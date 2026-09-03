import { readdir, readFile, stat } from "node:fs/promises"
import { resolve } from "node:path"
import * as v from "valibot"

import {
  marketingPagePaths,
  spaSurfaceRoots,
  staticPublicPages,
} from "../src/siteSurfaces"

const defaultBuildRoot = resolve(import.meta.dirname, "../dist")

/* Both lists are the site structure `src/siteSurfaces.ts` already declares.
   Restating them here let the robots check drift into covering four of the six
   application roots. */
const applicationEntries = spaSurfaceRoots.map(builtDocument)

function builtDocument(pathname: string) {
  const surface = pathname.replace(/^\/+|\/+$/g, "")
  return surface ? `${surface}/index.html` : "index.html"
}
const webManifestSchema = v.object({
  background_color: v.string(),
  description: v.string(),
  display: v.string(),
  icons: v.array(v.object({ src: v.string() })),
  id: v.string(),
  name: v.string(),
  scope: v.string(),
  short_name: v.string(),
  start_url: v.string(),
  theme_color: v.string(),
})
export async function verifyPublicBuild(
  buildRoot = defaultBuildRoot,
  publicOrigin = process.env.PUBLIC_SITE_ORIGIN ?? "http://127.0.0.1:5173",
  firebaseApiKey = process.env.VITE_FIREBASE_API_KEY,
) {
  const documents = await Promise.all(
    marketingPagePaths.map(async (pathname) => ({
      file: builtDocument(pathname),
      html: await readFile(resolve(buildRoot, builtDocument(pathname)), "utf8"),
      pathname,
    })),
  )
  const applicationDocuments = await Promise.all(
    applicationEntries.map(async (file) => ({
      file,
      html: await readFile(resolve(buildRoot, file), "utf8"),
    })),
  )
  const manifestReferences = await Promise.all(
    [...documents, ...applicationDocuments].map(({ file, html }) =>
      verifyBrandMetadata(buildRoot, html, file),
    ),
  )
  const uniqueManifestReferences = new Set(manifestReferences)
  if (uniqueManifestReferences.size !== 1) {
    throw new Error("Central Host pages must share one web app manifest")
  }
  await verifyWebManifest(buildRoot, manifestReferences[0] ?? "")

  /* A self-hosted instance names its own origin. Nothing here is indexable by
     default: an instance that wants search engines sets PUBLIC_SITE_ORIGIN. */
  const publicPagesAreIndexable = Boolean(process.env.PUBLIC_SITE_ORIGIN)
  const robots = /<meta name="robots" content="noindex,nofollow"\s*\/?>/i
  for (const { file, html, pathname } of documents) {
    if (html.trim().length === 0) {
      throw new Error(`${file} must emit a non-empty public page`)
    }
    if (publicPagesAreIndexable) {
      if (robots.test(html)) {
        throw new Error(`${file} must drop noindex in production`)
      }
    } else {
      requireMatch(html, robots, file)
    }
    requireMatch(
      html,
      /<meta name="referrer" content="no-referrer"\s*\/?>/i,
      file,
    )
    requireMatch(html, /<link rel="stylesheet"[^>]+href="\/[^"]+\.css">/i, file)
    requireMatch(html, /<h1\b/i, file)
    for (const route of staticPublicPages) {
      requireMatch(html, new RegExp(`href="/${route}/"`, "i"), file)
    }
    // Self-referencing: an alternation over every marketing path would let
    // `privacy/index.html` pass while carrying the landing's canonical.
    requireMatch(
      html,
      new RegExp(
        `<link rel="canonical" href="${escapeRegExp(`${publicOrigin}${pathname}`)}"`,
        "i",
      ),
      file,
    )
    requireMatch(html, /<meta property="og:title"/i, file)
    requireMatch(html, /<meta property="og:url"/i, file)
    requireMatch(html, /<meta name="twitter:card" content="summary"/i, file)
    if (file !== "index.html" && /<script\b/i.test(html)) {
      throw new Error(`${file} must remain readable without JavaScript`)
    }
    rejectFilesystemUrls(html, file)
    rejectOffOriginResources(html, file, publicOrigin)

    /* The root has to say what this is and offer a way in without running
       JavaScript, so a reader who blocks scripts still gets a page. */
    if (pathname === "/") {
      requireMatch(html, /href="\/app\/board"/i, file)
    }
  }

  for (const { file, html } of applicationDocuments) {
    requireMatch(html, /<div id="root"><\/div>/i, file)
    requireMatch(
      html,
      /<script type="module"[^>]+src="\/assets\/[^"]+\.js"><\/script>/i,
      file,
    )
    rejectFilesystemUrls(html, file)
    requireMatch(
      html,
      /<meta name="robots" content="noindex,nofollow"\s*\/?>/i,
      file,
    )
    requireMatch(
      html,
      /<meta name="referrer" content="no-referrer"\s*\/?>/i,
      file,
    )
  }

  await verifySitemap(buildRoot, publicOrigin)
  await verifyRobots(buildRoot)
  await verifyBrandedNotFound(buildRoot, publicOrigin)
  await verifyFirebaseConfigInlined(buildRoot, firebaseApiKey)
}

/** The application entries read `VITE_FIREBASE_*` off `import.meta.env`, which
 * only exists in the bundle if the build exposed that prefix. A build that
 * drops it still emits every page, so the authenticated routes render the
 * setup-required gate instead of signing anyone in. Only a build given the key
 * can prove it survived, so this is silent locally and live on the deploy. */
async function verifyFirebaseConfigInlined(
  buildRoot: string,
  firebaseApiKey: string | undefined,
) {
  const apiKey = firebaseApiKey?.trim()
  if (!apiKey) return
  const assetRoot = resolve(buildRoot, "assets")
  const scripts = (await readdir(assetRoot, { recursive: true })).filter(
    (file) => file.endsWith(".js"),
  )
  for (const file of scripts) {
    const source = await readFile(resolve(assetRoot, file), "utf8")
    if (source.includes(apiKey)) return
  }
  throw new Error(
    "no built bundle carries VITE_FIREBASE_API_KEY — the client build dropped the VITE_ env prefix",
  )
}

async function verifyWebManifest(buildRoot: string, reference: string) {
  const manifest = v.parse(
    webManifestSchema,
    JSON.parse(
      await readFile(resolveBuildReference(buildRoot, reference), "utf8"),
    ) as unknown,
  )
  if (
    manifest.name !== "ChenChess" ||
    manifest.short_name !== "ChenChess" ||
    manifest.description.length === 0 ||
    manifest.id !== "/app/board" ||
    manifest.start_url !== "/app/board" ||
    manifest.scope !== "/" ||
    manifest.display.length === 0 ||
    manifest.background_color.length === 0 ||
    manifest.theme_color.length === 0 ||
    manifest.icons.length === 0
  ) {
    throw new Error("web app manifest has invalid ChenChess metadata")
  }
  try {
    await Promise.all(
      manifest.icons.map(({ src }) => requireBuiltFile(buildRoot, src)),
    )
  } catch {
    throw new Error("web app manifest references a missing local icon")
  }
}

async function verifyBrandMetadata(
  buildRoot: string,
  html: string,
  file: string,
) {
  const links = (html.match(/<link\b[^>]*>/gi) ?? []).map(tagAttributes)
  const metas = (html.match(/<meta\b[^>]*>/gi) ?? []).map(tagAttributes)
  const manifestLinks = links.filter(({ rel }) =>
    relTokens(rel).has("manifest"),
  )
  const faviconLinks = links.filter(({ rel }) => relTokens(rel).has("icon"))
  const touchIconLinks = links.filter(({ rel }) =>
    relTokens(rel).has("apple-touch-icon"),
  )
  const themeColors = metas.filter(
    ({ name }) => name?.toLowerCase() === "theme-color",
  )
  if (
    manifestLinks.length !== 1 ||
    !faviconLinks.some(({ media }) => media?.includes("color-scheme: light")) ||
    !faviconLinks.some(({ media }) => media?.includes("color-scheme: dark")) ||
    touchIconLinks.length === 0 ||
    !themeColors.some(({ media }) => media?.includes("color-scheme: light")) ||
    !themeColors.some(({ media }) => media?.includes("color-scheme: dark"))
  ) {
    throw new Error(`${file} is missing required static public-page structure`)
  }
  const references = [...manifestLinks, ...faviconLinks, ...touchIconLinks].map(
    ({ href }) => href ?? "",
  )
  try {
    await Promise.all(
      references.map((reference) => requireBuiltFile(buildRoot, reference)),
    )
  } catch {
    throw new Error(`${file} references a missing local brand asset`)
  }
  return manifestLinks[0]?.href ?? ""
}

function tagAttributes(tag: string) {
  return {
    href: attribute(tag, "href"),
    media: attribute(tag, "media"),
    name: attribute(tag, "name"),
    rel: attribute(tag, "rel"),
  }
}

function attribute(tag: string, name: string) {
  return tag
    .match(new RegExp(`\\b${name}=(?:"([^"]*)"|'([^']*)')`, "i"))
    ?.slice(1)
    .find((value) => value !== undefined)
}

function relTokens(value: string | undefined) {
  return new Set(value?.toLowerCase().split(/\s+/) ?? [])
}

async function requireBuiltFile(buildRoot: string, reference: string) {
  const metadata = await stat(resolveBuildReference(buildRoot, reference))
  if (!metadata.isFile()) throw new Error("not a file")
}

function resolveBuildReference(buildRoot: string, reference: string) {
  return resolve(buildRoot, reference.replace(/^\/+/, ""))
}

async function verifySitemap(buildRoot: string, publicOrigin: string) {
  const xml = await readFile(resolve(buildRoot, "sitemap.xml"), "utf8")
  const expected = marketingPagePaths.map((path) => `${publicOrigin}${path}`)
  for (const loc of expected) {
    if (!xml.includes(`<loc>${loc}</loc>`)) {
      throw new Error(`sitemap.xml must list ${loc}`)
    }
  }
  const locs = [...xml.matchAll(/<loc>([^<]+)<\/loc>/g)].map(
    (match) => match[1] ?? "",
  )
  if (locs.length !== expected.length) {
    throw new Error("sitemap.xml must list the marketing pages only")
  }
}

async function verifyRobots(buildRoot: string) {
  const robots = await readFile(resolve(buildRoot, "robots.txt"), "utf8")
  for (const path of spaSurfaceRoots) {
    if (!robots.includes(`Disallow: ${path}`)) {
      throw new Error(`robots.txt must disallow ${path}`)
    }
  }
}

async function verifyBrandedNotFound(buildRoot: string, publicOrigin: string) {
  const html = await readFile(resolve(buildRoot, "404.html"), "utf8")
  if (html.trim().length === 0) {
    throw new Error("404.html must emit a non-empty public page")
  }
  requireMatch(html, /<h1\b/i, "404.html")
  requireMatch(
    html,
    /<meta name="referrer" content="no-referrer"\s*\/?>/i,
    "404.html",
  )
  requireMatch(
    html,
    /<meta name="robots" content="noindex,nofollow"\s*\/?>/i,
    "404.html",
  )
  requireMatch(
    html,
    /<link rel="stylesheet"[^>]+href="\/[^"]+\.css">/i,
    "404.html",
  )
  rejectFilesystemUrls(html, "404.html")
  rejectOffOriginResources(html, "404.html", publicOrigin)
}

function rejectFilesystemUrls(html: string, file: string) {
  if (/\bfile:/i.test(html)) {
    throw new Error(`${file} must not emit filesystem asset URLs`)
  }
}

function rejectOffOriginResources(
  html: string,
  file: string,
  publicOrigin: string,
) {
  if (
    /<(?:iframe|object|embed)\b/i.test(html) ||
    /(?:googletagmanager|google-analytics|hotjar|posthog|segment\.com)/i.test(
      html,
    )
  ) {
    throw new Error(`${file} must not contain third-party tracking resources`)
  }
  const origin = publicOrigin.replace(/\/+$/, "")
  // The canonical link is the one tag that has to carry an absolute URL. Every
  // other absolute reference is rejected whatever its origin, so a same-origin
  // script or stylesheet cannot slip in behind the canonical's exemption.
  for (const tag of html.match(/<(?:script|img|source|link)\b[^>]*>/gi) ?? []) {
    const reference = attribute(tag, "src") ?? attribute(tag, "href")
    if (!reference || !/^(?:https?:)?\/\//i.test(reference)) continue
    const isSelfCanonical =
      relTokens(attribute(tag, "rel")).has("canonical") &&
      (reference === origin || reference.startsWith(`${origin}/`))
    if (isSelfCanonical) continue
    throw new Error(`${file} must not reference ${reference} off the page`)
  }
}

function escapeRegExp(value: string) {
  return value.replaceAll(/[.*+?^${}()|[\]\\]/g, "\\$&")
}

function requireMatch(value: string, pattern: RegExp, file: string) {
  if (!pattern.test(value)) {
    throw new Error(`${file} is missing required static public-page structure`)
  }
}

if (import.meta.main) {
  try {
    await verifyPublicBuild()
    process.stdout.write("verified static public build output\n")
  } catch (error) {
    process.stderr.write(
      `${error instanceof Error ? error.message : "public build verification failed"}\n`,
    )
    process.exitCode = 1
  }
}
