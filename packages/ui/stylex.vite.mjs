import stylex from "@stylexjs/unplugin"

import { chenCascadeLayers } from "./src/styles/cascadeLayers.mjs"

/**
 * The StyleX compiler every ChenChess Vite graph must run. Astryx ships
 * pre-compiled, but ChenChess authors `stylex.create` / `xstyle` — without this
 * plugin those styles compile as JS and render with no CSS.
 *
 * StyleX layers sit after every named ChenChess / Astryx layer so authored
 * overrides can win without becoming unlayered (unlayered CSS beats every
 * layer, which is how a closed dialog stayed laid out).
 *
 * This file is plain ESM so Vite and Vitest configs can import it through
 * `@chenchess/ui/stylex.vite` without asking Node to load TypeScript.
 */
export function chenStylexVitePlugin() {
  const stylexVite = stylexPlugin()
  return [
    stylexVite,
    // Build only: Vite's own css-post plugin emits the CSS asset after the
    // StyleX plugin's pre-phase generateBundle has already looked for one, so
    // the compiled craft ended up in a stray assets/stylex.css that
    // vite-plugin-singlefile never inlines. This post-phase bridge appends the
    // collected CSS to the real asset before any later post plugin (the
    // single-file inliner) consumes it. Keep it ahead of viteSingleFile.
    {
      name: "chen-stylex-css-into-bundle",
      apply: "build",
      enforce: "post",
      generateBundle(_options, bundle) {
        const css = stylexVite.__stylexCollectCss?.()
        if (!css) return
        // Multi-page builds emit one CSS asset per entry and each page loads
        // only its own, so the collected StyleX must ride every asset.
        for (const asset of Object.values(bundle)) {
          if (asset.type !== "asset" || !asset.fileName.endsWith(".css")) {
            continue
          }
          // Rollup asset sources are string | Uint8Array; CSS assets are text.
          const current = String(asset.source)
          if (!current.includes(css)) asset.source = `${current}\n${css}`
        }
      },
    },
    // Dev only: the unplugin serves authored StyleX through a virtual
    // stylesheet that the HTML shell must reference; builds emit real CSS
    // assets instead. Without this tag every authored style silently
    // disappears from `vite dev` while looking fine in production.
    {
      name: "chen-stylex-dev-css",
      apply: "serve",
      transformIndexHtml() {
        return [
          {
            attrs: { href: "/virtual:stylex.css", rel: "stylesheet" },
            injectTo: "head",
            tag: "link",
          },
          {
            attrs: { src: "/@id/virtual:stylex:runtime", type: "module" },
            injectTo: "head",
            tag: "script",
          },
        ]
      },
    },
  ]
}

function stylexPlugin() {
  return stylex.vite({
    // Vitest has no HTML shell to load the StyleX HMR runtime; leaving
    // `devMode` at `full` keeps a Vite server handle open after the suite.
    devMode: process.env.VITEST === "true" ? "off" : "full",
    // The last-media-query-wins transform parses only range features and
    // throws on `(prefers-reduced-motion: reduce)`, which the watercolor
    // craft conditions on. Authored order already puts overrides last.
    enableMediaQueryOrder: false,
    useCSSLayers: {
      before: [...chenCascadeLayers],
      prefix: "stylex",
    },
  })
}
