# ChenChess Oxlint anti-slop

Adapted from [`dmmulroy/anti-slop`](https://github.com/dmmulroy/anti-slop)
at `6d538555cb151d4121ed51a27db81890eacf8ae9`. The upstream project is meant
to be copied and changed. This directory is the ChenChess copy.

## ChenChess adaptations

- Unknown stays legal on named boundary parsers (`parse*`, `decode*`,
  `assert*`, `read*`, `from*`) and type predicates. Internal functions still
  take a named owner type.
- `typeof` is allowed inside those same parsers and type predicates
  (`no-runtime-typeof` defaults). Pass both options `false` for upstream-strict.
- Prefer a Valibot schema and named `parse*` / `from*` / `read*`
  constructors over `// SAFETY:` plus `as Owner`. Prefer Valibot over
  Zod for new schemas. A `typeof` wrapper is not a schema.
- `as unknown` does not need a `SAFETY:` comment. It widens to the untrusted
  boundary. Assertions to a domain type still do. Use SAFETY only when a
  constructor cannot own the type.
- `value as unknown as Owner` is a permitted assertion chain. `as object as T`
  is not.
- `error` and `err` are allowed unknown parameters, matching `cause`.
- Effect rules are not copied. ChenChess has no direct Effect dependency.
- Root Oxlint enables every default anti-slop rule as `"error"`. Do not
  add file overrides to silence a finding; fix the contract. The
  `typed-evidence` skill and `CODING_STANDARDS.md` list the
  ChenChess carve-outs (`as unknown as Owner`, named `parse*` boundaries,
  `error`/`err`/`cause`).
- `plugin.js` is the generated Oxlint entry. Rebuild it with
  `bun run --cwd tooling/oxlint build` after changing rule source.
  `bun run --cwd tooling/oxlint check` fails if the committed bundle drifted.

The Rust mapping lives in `CODING_STANDARDS.md` and the
`typed-evidence` skill. Rebuild: `docs/agents/typed-evidence.md`.
