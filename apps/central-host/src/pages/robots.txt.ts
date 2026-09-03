import type { APIRoute } from "astro"

import { spaSurfaceRoots } from "../siteSurfaces"
import { canonicalUrl } from "../siteOrigin"

export const GET: APIRoute = () => {
  const disallows = spaSurfaceRoots
    .map((root) => `Disallow: ${root}`)
    .join("\n")
  return new Response(
    `User-agent: *\n${disallows}\nSitemap: ${canonicalUrl("/sitemap.xml")}\n`,
    {
      headers: { "Content-Type": "text/plain; charset=utf-8" },
    },
  )
}
