/**
 * Asset imports resolve to their served URL.
 *
 * Ambient declarations reach a program only through `include`, and every
 * consumer compiles `packages/ui/src` through a path alias rather than this
 * package's own tsconfig. Each program that pulls in `assets.ts` therefore
 * names this file: `apps/central-host/tsconfig.app.json`,
 * `apps/central-host/tsconfig.server.json`, `packages/coach-fixtures`, and
 * `tooling/scripts`.
 * Only `?url` is declared for SVG:
 * a bare `*.svg` import is an Astro component inside the Central Host graph,
 * not a string, and rendered straight into a CSS `url()` it produced
 * `url("(...args) => …")` on every prerendered board piece.
 */
declare module "*.svg?url" {
  const source: string
  export default source
}

declare module "*.svg?raw" {
  const source: string
  export default source
}

declare module "*.webp?url" {
  const source: string
  export default source
}
