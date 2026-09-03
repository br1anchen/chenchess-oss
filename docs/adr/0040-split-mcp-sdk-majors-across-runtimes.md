# Split MCP SDK majors across runtimes

## Status

Accepted.

## Context

ChenChess resolves two incompatible majors of the MCP SDK at once. The Central
Host server depends on `@modelcontextprotocol/server`, `/node`, `/express`, and
`/client` at 2.0.0. The Coach App widget depends on
`@modelcontextprotocol/ext-apps@1.7.5`, whose peer range is
`@modelcontextprotocol/sdk@^1.29.0` — the v1 line.

Two majors in one lockfile normally reads as an unfinished migration. Here it is
not one. The MCP Apps extension has its own release timeline and has not shipped
against the v2 packages, so the widget cannot move until it does; and the two
consumers are separate runtimes — a browser bundle built from
`apps/coach-app/src/main.tsx`, and a Node server bundled from
`apps/central-host/production.ts` — that never share a module graph. The widget
reaches the Central Host only as a build input: `@chenchess/coach-app` is a
`devDependency` that produces prebuilt single-file HTML artifacts, and the
server bundles with `--packages=external`, so nothing the widget resolves is
loadable from the deployed server.

That isolation was an assumption. Nothing enforced it, so an ordinary dependency
addition could have pulled the v1 SDK into the server's resolved tree, where the
two majors would compete for the same import specifiers.

## Decision

Keep the two majors. Do not migrate the widget to v2 ahead of `ext-apps`, and do
not downgrade the Central Host to v1 to unify them.

Enforce the isolation instead of assuming it.
`tooling/scripts/mcp-sdk-major-split.test.ts` runs in the ordinary
`bun run test` suite and resolves each workspace's **production** dependency
closure from disk — its `dependencies`, transitively, plus every installed
optional dependency and peer. Copies are keyed by resolved directory, so a
nested second copy of an already-seen package name cannot mask a different
major. It fails when:

- the Central Host closure contains `@modelcontextprotocol/sdk`, or any
  `@modelcontextprotocol/*` package below major 2;
- the Central Host closure contains no MCP package at all, which would make the
  first assertion pass vacuously;
- the widget stops resolving MCP SDK v1, which retires the premise of this ADR
  and should be a deliberate change to it; or
- the widget and the Central Host closures share any `@modelcontextprotocol/*`
  package.

## Measured widget cost

Measured once, 2026-08-07, at `@modelcontextprotocol/ext-apps@1.7.5` and
`@modelcontextprotocol/sdk@1.29.0`, against the built Critical Moment Selector
artifact. Reproduce with:

```sh
bun run --filter @chenchess/coach-app build
bun run --filter @chenchess/coach-app report:bundle
```

`reportBundle.ts` attributes `ext-apps`, `@modelcontextprotocol/sdk`, and `zod`
to a single `mcp-apps-runtime` category. Nothing else in the widget imports zod
or the SDK, so that category is exactly the v1 chain's cost:

| Measure                        | Bytes     |
| ------------------------------ | --------- |
| Delivered artifact, raw        | 1,095,930 |
| Delivered artifact, gzip       | 389,393   |
| `mcp-apps-runtime`, attributed | 280,298   |
| `mcp-apps-runtime`, est. gzip  | 99,592    |

The v1 chain is **~26% of the delivered gzipped widget**. Splitting the category
by package, in minified esbuild `bytesInOutput` over the 899,366 analyzed script
bytes:

| Package                          | Minified bytes | Share of the chain |
| -------------------------------- | -------------- | ------------------ |
| `zod` (v3 compat plus v4 core)   | 333,923        | 85.1%              |
| `@modelcontextprotocol/ext-apps` | 29,435         | 7.5%               |
| `@modelcontextprotocol/sdk`      | 29,057         | 7.4%               |

The dominant cost is not the SDK — it is zod, which no ChenChess source imports.
It arrives through `ext-apps/dist/src/app.js` →
`sdk/dist/esm/shared/protocol.js` and `sdk/dist/esm/types.js`, which pull both
the v3 compatibility surface and the v4 core, including every bundled locale.

## Consequences

The split is now a CI-enforced property, so the Central Host cannot acquire the
v1 SDK by an unnoticed transitive edge. Ordinary dependency changes rerun the
assertion through `bun.lock` and the two app manifests being declared inputs of
`@chenchess/scripts#test`.

The measurement gives the widget rewrite in
#258 a concrete number to
work against: the provisional 150 KB gzip objective in `reportBundle.ts` is
already exceeded at 389 KB, and a quarter of that is the v1 chain. Reducing it
is a dependency question — whether `ext-apps` can avoid dragging zod's full
locale set into a browser bundle — not a product-code question. This ADR does
not commit to that work.

When `ext-apps` releases against MCP SDK v2, the widget migration will trip the
"widget stops resolving MCP SDK v1" assertion. That failure is the signal to
revisit this ADR, not a defect to patch around.
