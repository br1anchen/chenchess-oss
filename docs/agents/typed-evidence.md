# Typed evidence

The live TypeScript Oxlint floor, boundary pattern, and Coach Engine Rust
mapping live in `CODING_STANDARDS.md`. This file is the rebuild and
ownership how-to.

ChenChess adapts [`dmmulroy/anti-slop`](https://github.com/dmmulroy/anti-slop)
as owned Oxlint rules in `tooling/oxlint`. The plugin is not a frozen
vendor copy. Upstream says to copy the rules and change them.

`deslop` still owns generated prose and UI slop. `defensive-code-cleaner`
owns impossible-state guards. `grill-with-types` owns type and file design.

## Rebuild

Root `.oxlintrc.json` loads `./tooling/oxlint/plugin.js` as `anti-slop`.
`bun run lint` is the gate. Plugin unit tests live in
`tooling/oxlint/rules/*.test.ts` and `tooling/oxlint/shared/*.test.ts`.
After changing rule source, rebuild with `bun run --cwd tooling/oxlint build`.
`bun run --cwd tooling/oxlint check` typechecks, runs those tests, and
fails if `plugin.js` drifted from a fresh bundle.

Do not disable a rule to silence a finding; fix the contract. The error
floor is every default anti-slop rule, all `"error"`.
