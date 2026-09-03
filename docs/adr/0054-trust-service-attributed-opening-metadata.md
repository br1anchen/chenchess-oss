# Trust service-attributed opening metadata without re-fetching

Accepted. Renumbered from 0026 so that number stays with the Move Intent
lifecycle decision.

ChenChess treats a complete opening name and ECO as authoritative when supplied by a direct Lichess or Chess.com import, or by a PGN with one unambiguous, syntactically valid service Game URL in its `Site` or `Link` header, and prefers that identification to the bundled Opening Catalog. V1 deliberately does not re-fetch a PGN-attributed Game because stronger provenance would prolong and make the import pipeline network-dependent; typed attribution records the weaker trust boundary, while incomplete or ambiguous metadata falls back to the versioned catalog. Learning resources still require an explicit, release-verified mapping from the exact service identity, so this choice never permits fuzzy matching, generated URLs, or runtime discovery.
