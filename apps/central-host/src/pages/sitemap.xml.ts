import type { APIRoute } from "astro"

import { marketingPagePaths } from "../siteSurfaces"
import { canonicalUrl } from "../siteOrigin"

export const GET: APIRoute = () => {
  const urls = marketingPagePaths
    .map((pathname) => `  <url><loc>${canonicalUrl(pathname)}</loc></url>`)
    .join("\n")
  return new Response(
    `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`,
    {
      headers: { "Content-Type": "application/xml; charset=utf-8" },
    },
  )
}
