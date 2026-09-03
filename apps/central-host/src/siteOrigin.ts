/**
 * Where this instance is served from.
 *
 * A self-hosted instance names its own origin; nothing here carries a
 * deployment's identity. The default is the local development origin, so a
 * clean clone builds and serves without any configuration at all.
 */
const defaultSiteOrigin = "http://127.0.0.1:5173"

function siteOrigin() {
  const configured = process.env.PUBLIC_SITE_ORIGIN?.trim()
  if (!configured) return defaultSiteOrigin
  return configured.endsWith("/") ? configured.slice(0, -1) : configured
}

export function isProductionSite() {
  return Boolean(process.env.PUBLIC_SITE_ORIGIN)
}

/** Every marketing path already carries its trailing slash, `/` included. */
export function canonicalUrl(pathname: string) {
  return `${siteOrigin()}${pathname}`
}
