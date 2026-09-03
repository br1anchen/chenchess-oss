/**
 * Canonical cascade layer order. `layers.css` declares the same list as an
 * `@layer` at-rule, and `chenStylexVitePlugin` repeats it in
 * `useCSSLayers.before`. Keep both consumers on this export.
 */
export const chenCascadeLayers = [
  "reset",
  "chen-tokens",
  "chen-base",
  "astryx-base",
  "astryx-theme",
  "surfaces",
]
