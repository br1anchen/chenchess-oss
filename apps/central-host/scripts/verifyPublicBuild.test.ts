import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { dirname, join } from "node:path"
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest"

import { marketingPagePaths, spaSurfaceRoots } from "../src/siteSurfaces"
import { verifyPublicBuild } from "./verifyPublicBuild"

const siteOrigin = "https://chess.example"
const temporaryRoots: string[] = []

/* The fixtures below are page trees, not builds — they carry no `assets/`.
   The Firebase inlining check is silent without a key, so on a machine that
   has one configured (`apps/central-host/.env.local`) these tests would go
   looking for bundles the fixture never had. A test states the environment it
   runs in rather than inheriting the developer's. */
beforeEach(() => {
  vi.stubEnv("VITE_FIREBASE_API_KEY", "")
})

afterEach(async () => {
  vi.unstubAllEnvs()
  await Promise.all(
    temporaryRoots
      .splice(0)
      .map((root) => rm(root, { force: true, recursive: true })),
  )
})

describe("static public build verification", () => {
  test("accepts a sign-in-first landing page and static legal pages", async () => {
    const root = await publicBuildFixture()

    await expect(verifyPublicBuild(root, siteOrigin)).resolves.toBeUndefined()
  })

  test("rejects a web manifest with one environment hard-coded into its identity", async () => {
    const root = await publicBuildFixture()
    await writeFile(
      join(root, "site.webmanifest"),
      JSON.stringify({
        ...webManifest(),
        id: "https://chess.example/app/",
      }),
    )

    await expect(verifyPublicBuild(root, siteOrigin)).rejects.toThrow(
      "web app manifest has invalid ChenChess metadata",
    )
  })

  test("rejects an icon reference whose asset is absent from the build", async () => {
    const root = await publicBuildFixture()
    const referencedIcon = webManifest().icons.find(
      ({ src }) => !brandHead().includes(src),
    )?.src
    if (!referencedIcon) throw new Error("fixture must reference an icon")
    await rm(join(root, referencedIcon), { force: true })

    await expect(verifyPublicBuild(root, siteOrigin)).rejects.toThrow(
      "web app manifest references a missing local icon",
    )
  })

  test("rejects JavaScript on a public page", async () => {
    const root = await publicBuildFixture()
    await writePublicPage(
      root,
      "privacy/index.html",
      '<h1>Privacy</h1><script type="module" src="/assets/firebase.js"></script>',
    )

    await expect(verifyPublicBuild(root, siteOrigin)).rejects.toThrow(
      "privacy/index.html must remain readable without JavaScript",
    )
  })

  test("rejects an invalid application entry", async () => {
    const root = await publicBuildFixture()
    await writeFile(join(root, "app/index.html"), "App placeholder")

    await expect(verifyPublicBuild(root, siteOrigin)).rejects.toThrow(
      "app/index.html is missing required static public-page structure",
    )
  })

  test("requires a named instance to drop noindex on its public pages", async () => {
    const root = await publicBuildFixture()

    vi.stubEnv("PUBLIC_SITE_ORIGIN", siteOrigin)
    await expect(verifyPublicBuild(root, siteOrigin)).rejects.toThrow(
      "index.html must drop noindex in production",
    )
  })

  test("accepts indexable public pages on a named instance", async () => {
    const root = await publicBuildFixture({
      indexPublicPages: true,
      origin: siteOrigin,
    })

    vi.stubEnv("PUBLIC_SITE_ORIGIN", siteOrigin)
    await expect(verifyPublicBuild(root, siteOrigin)).resolves.toBeUndefined()
  })

  test("rejects a sitemap that lists an application path", async () => {
    const root = await publicBuildFixture()
    await writeFile(
      join(root, "sitemap.xml"),
      `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://chess.example/</loc></url>
  <url><loc>https://chess.example/privacy/</loc></url>
  <url><loc>https://chess.example/support/</loc></url>
  <url><loc>https://chess.example/terms/</loc></url>
  <url><loc>https://chess.example/app/</loc></url>
</urlset>
`,
    )

    await expect(verifyPublicBuild(root, siteOrigin)).rejects.toThrow(
      "sitemap.xml must list the marketing pages only",
    )
  })

  test("rejects robots.txt that leaves /app crawlable", async () => {
    const root = await publicBuildFixture()
    await writeFile(join(root, "robots.txt"), "User-agent: *\nAllow: /\n")

    await expect(verifyPublicBuild(root, siteOrigin)).rejects.toThrow(
      `robots.txt must disallow ${spaSurfaceRoots[0] ?? ""}`,
    )
  })

  test("rejects a filesystem brand URL on a public page", async () => {
    const root = await publicBuildFixture()
    await writePublicPage(
      root,
      "privacy/index.html",
      '<h1>Privacy</h1><img src="file:///workspace/dist/chunks/app-icon-light.svg">',
    )

    await expect(verifyPublicBuild(root, siteOrigin)).rejects.toThrow(
      "privacy/index.html must not emit filesystem asset URLs",
    )
  })

  test("rejects a protocol-relative application script", async () => {
    const root = await publicBuildFixture()
    await writeFile(
      join(root, "app/index.html"),
      `${brandHead()}<meta name="robots" content="noindex,nofollow"><meta name="referrer" content="no-referrer"><div id="root"></div><script type="module" src="//assets/app.js"></script>`,
    )

    await expect(verifyPublicBuild(root, siteOrigin)).rejects.toThrow(
      "app/index.html is missing required static public-page structure",
    )
  })

  test("rejects a third-party tracking resource", async () => {
    const root = await publicBuildFixture()
    await writePublicPage(
      root,
      "support/index.html",
      `${supportContent()}<img src="https://tracking.example/pixel">`,
    )

    await expect(verifyPublicBuild(root, siteOrigin)).rejects.toThrow(
      "support/index.html must not reference https://tracking.example/pixel off the page",
    )
  })
})

async function publicBuildFixture({
  indexPublicPages = false,
  origin = "https://chess.example",
}: {
  indexPublicPages?: boolean
  origin?: string
} = {}) {
  const root = await mkdtemp(join(tmpdir(), "chenchess-public-build-"))
  temporaryRoots.push(root)
  await Promise.all([
    writePublicPage(
      root,
      "index.html",
      '<h1>ChenChess</h1><a href="/app/board">Open the Coaching Board</a>',
      { noindex: !indexPublicPages, origin },
    ),
    writePublicPage(root, "privacy/index.html", privacyContent(), {
      noindex: !indexPublicPages,
      origin,
    }),
    writePublicPage(root, "support/index.html", supportContent(), {
      noindex: !indexPublicPages,
      origin,
    }),
    writePublicPage(root, "terms/index.html", "<h1>Terms</h1>", {
      noindex: !indexPublicPages,
      origin,
    }),
  ])
  for (const entry of ["app", "join", "login"]) {
    await mkdir(join(root, entry), { recursive: true })
    await writeFile(
      join(root, entry, "index.html"),
      `${brandHead()}<meta name="robots" content="noindex,nofollow"><meta name="referrer" content="no-referrer"><div id="root"></div><script type="module" src="/assets/app.js"></script>`,
    )
  }
  await writeBrandAssets(root)
  await writeFile(
    join(root, "sitemap.xml"),
    `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>${origin}/</loc></url>
  <url><loc>${origin}/privacy/</loc></url>
  <url><loc>${origin}/support/</loc></url>
  <url><loc>${origin}/terms/</loc></url>
</urlset>
`,
  )
  await writeFile(
    join(root, "robots.txt"),
    `User-agent: *\n${spaSurfaceRoots.map((root) => `Disallow: ${root}`).join("\n")}\n`,
  )
  await writePublicPage(root, "404.html", "<h1>Page not found</h1>", {
    noindex: true,
    origin,
  })
  return root
}

function privacyContent() {
  return "<h1>Privacy</h1>"
}

function supportContent() {
  return "<h1>Support</h1>"
}

async function writePublicPage(
  root: string,
  file: string,
  content: string,
  {
    canonical,
    noindex = true,
    origin = "https://chess.example",
  }: { canonical?: string; noindex?: boolean; origin?: string } = {},
) {
  const path = join(root, file)
  await mkdir(dirname(path), { recursive: true })
  const robots = noindex
    ? '<meta name="robots" content="noindex,nofollow">'
    : ""
  const canonicalUrl = canonical ?? canonicalFor(file, origin)
  await writeFile(
    path,
    `${brandHead()}${robots}<link rel="canonical" href="${canonicalUrl}"><meta property="og:title" content="ChenChess"><meta property="og:url" content="${canonicalUrl}"><meta name="twitter:card" content="summary"><meta name="referrer" content="no-referrer"><link rel="stylesheet" href="/assets/public.css"><a href="/privacy/">Privacy</a><a href="/support/">Support</a><a href="/terms/">Terms</a>${content}`,
  )
}

function canonicalFor(file: string, origin: string) {
  const pathname = marketingPagePaths.find(
    (candidate) => file === `${candidate.replace(/^\/+|\/+$/g, "")}/index.html`,
  )
  return `${origin}${pathname ?? "/"}`
}

function brandHead() {
  return [
    '<link rel="manifest" href="/site.webmanifest">',
    '<link rel="icon" type="image/svg+xml" media="(prefers-color-scheme: light)" href="/brand/app-icon-light.svg">',
    '<link rel="icon" type="image/svg+xml" media="(prefers-color-scheme: dark)" href="/brand/app-icon-dark.svg">',
    '<link rel="apple-touch-icon" sizes="180x180" href="/brand/app-icon-light-180.png">',
    '<meta name="theme-color" media="(prefers-color-scheme: light)" content="#F7F2E8">',
    '<meta name="theme-color" media="(prefers-color-scheme: dark)" content="#142B46">',
  ].join("")
}

function webManifest() {
  return {
    background_color: "#F7F2E8",
    description:
      "Grounded chess game review on the web, and in ChatGPT and Claude.",
    display: "standalone",
    icons: [
      {
        purpose: "any",
        sizes: "any",
        src: "/brand/app-icon-light.svg",
        type: "image/svg+xml",
      },
      {
        purpose: "any",
        sizes: "512x512",
        src: "/brand/app-icon-light-512.png",
        type: "image/png",
      },
    ],
    id: "/app/board",
    name: "ChenChess",
    scope: "/",
    short_name: "ChenChess",
    start_url: "/app/board",
    theme_color: "#142B46",
  }
}

async function writeBrandAssets(root: string) {
  const linkedAssets = [
    ...webManifest().icons.map(({ src }) => src),
    ...[...brandHead().matchAll(/\bhref="([^"]+)"/g)].map(
      (match) => match[1] ?? "",
    ),
  ].filter((reference) => reference !== "/site.webmanifest")
  await Promise.all([
    writeFile(join(root, "site.webmanifest"), JSON.stringify(webManifest())),
    ...[...new Set(linkedAssets)].map(async (reference) => {
      const path = join(root, reference)
      await mkdir(dirname(path), { recursive: true })
      await writeFile(path, reference)
    }),
  ])
}

test("rejects a marketing page that canonicalises to another page", async () => {
  const root = await publicBuildFixture()
  await writePublicPage(root, "privacy/index.html", "<p>Privacy</p>", {
    canonical: "https://chess.example/",
  })

  await expect(verifyPublicBuild(root, siteOrigin)).rejects.toThrow(
    "privacy/index.html is missing required static public-page structure",
  )
})

test("rejects a same-origin absolute script on a marketing page", async () => {
  const root = await publicBuildFixture()
  await writePublicPage(
    root,
    "index.html",
    `<h1>ChenChess</h1><a href="/app/board">Open the Coaching Board</a><script src="https://chess.example/tag.js"></script>`,
  )

  await expect(verifyPublicBuild(root, siteOrigin)).rejects.toThrow(
    "index.html must not reference https://chess.example/tag.js off the page",
  )
})
