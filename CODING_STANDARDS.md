# Coding standards

ChenChess coding standards for agents and reviewers. Always-on safety,
skill catalog, and documentation placement stay in `AGENTS.md`. Domain
vocabulary stays in `CONTEXT.md`. This file is the engineering bar.

## Package boundaries

`apps/` and `services/` are deployable units. They do not import, `COPY`,
`include_str!`, or otherwise read each other's source, templates, fixtures,
or constants.

When two deployables need the same JSON, PGN, prompt sentence, or number,
put it in `packages/` and import that package from both sides.

### Allowed

| From                                | To                                                                |
| ----------------------------------- | ----------------------------------------------------------------- |
| `apps/*`, `services/*`, `tooling/*` | `packages/*`                                                      |
| `packages/*`                        | other `packages/*`                                                |
| `tooling/*`                         | `apps/*` or `services/*` for repo-level gates that inspect a unit |

Workspace `package.json` manifests may be copied into a Docker install
layer so `bun install` sees every workspace member. That is not a source
import.

### Not allowed

- `apps/central-host` importing `services/coach-engine/**`
- `services/coach-engine` `include_str!` or path-reading `apps/**`
- `packages/*` importing `apps/**` or `services/**`
- Docker `COPY services/...` into an `apps/` image except a workspace
  `package.json`
- Duplicating a sentence or constant in both trees and hoping they stay
  aligned

### Shared packages stay split by consumer

`@chenchess/shared-assets` and `@chenchess/coach-fixtures` are not one
package.

| Package                        | What it holds                                                                 | Who compiles it                                 |
| ------------------------------ | ----------------------------------------------------------------------------- | ----------------------------------------------- |
| `@chenchess/shared-assets`     | Canonical Game bytes, Grounding Gate sentences, shared limits                 | Coach Engine (`include_str!`) and Central Host  |
| `@chenchess/coach-fixtures`    | Coach App widget replay recordings and the TypeScript helpers that serve them | Coach App tests and the fixture MCP server      |
| `@chenchess/review-projection` | Pure projection from a contract into what a surface renders                   | Central Host, Coach App, and fixture generators |

Coach Engine's image copies `packages/shared-assets` so Rust can embed the
Canonical Game. It does not copy widget replay TypeScript or
`@chenchess/ui`. Folding those into `shared-assets` would pull Coach App
harness code into a crate that only needs JSON and PGN.

The sanitized preview JSON in `coach-fixtures` is a _projection_ of the
Canonical Game, not a second copy of it. The generator reads the shared
baseline from `shared-assets` and calls
`projectReviewSessionPresentation` from `review-projection`.

### Adding a shared asset

1. Put the file in `packages/shared-assets` (or a new `packages/` crate if
   the share is a different concern).
2. Export a typed parse from `src/index.ts`. Rust `include_str!` the same
   file via `CARGO_MANIFEST_DIR/../../packages/shared-assets/...`.
3. Point both deployables at the package. Do not add a `COPY` of a service
   path to an app image, and do not add a Railway watch path that reaches
   across `apps/` and `services/`.
4. Add the package to the consuming image (`COPY packages/<name>`), its
   Railway watch list, and `build:server` `--alias` when Central Host's
   bundled server imports it (`verifyServerBundle` fails if the alias is
   missing).

`apps/` and `services/` source files must not mention each other's trees, and
`packages/` must not import `apps/`. The gate that asserted this also asserted
the deployment topology, and went with it.

## Tests encode a live contract

A test earns its keep by failing when behavior drifts. Prefer:

- An independent oracle: a fixture, a Player-visible sentence, a decoded
  envelope, or a rejected input.
- Rejection and boundary cases over happy-path identity.
- Closed-list locks that fail when a variant is added.

Do not:

- Re-parse the same file the module already parsed and assert equality.
- Assert `fromX(y) === y` for branding or identity constructors.
- Assert `f(x) === f(x)` to "prove" purity.
- Restate a type guard after locking the closed list it membership-checks.
- Add a denylist whose only job is to reject a completed ADR cut.

Typed seams beat mocks. See Typed evidence. Do not add `vi.mock`.

### Completed ADR cuts are not test subjects

A superseded ADR is history. Do not add a test, script, CI gate, operator
flag, or source scan whose only job is to reject a retired name, command
tag, schema label, path, or one-time migration procedure.

Those checks are useful once, during the cut. After the live types and
callers no longer contain the retired form, they are noise: they mention
the old name, they fail for reasons unrelated to current behavior, and they
invite agents to keep extending the denylist.

Do:

- Encode the live contract. If `offerIntentHypothesis` is gone, the
  generated command union and its decoder already reject unknown kinds.
- Delete a one-time operator after the cut lands.
- Keep a check that still owns a current invariant even if an ADR
  introduced it (MCP `2026-07-28` negotiation, Coach Engine credentials
  out of Central Host, eight-tool Language Layer surface).

Do not:

- Add `expect(x).not.toContain("start_review_session")` or a loop over
  removed command tags.
- Add a lockfile or Dockerfile scan for a retired prototype path that no
  longer exists.
- Add a `--sweep-retired-*` flag, a one-time staging reset, or a leftover
  SemVer promotion helper "in case we need it."
- Treat `CONTEXT.md`'s **Do not restore** list as a test fixture. It is
  vocabulary. History lives in the ADR.

If a reviewer asks for a "do not regress to X" test after X is gone from
the type system, refuse and point here.

## Typed evidence

Parse untrusted input once at a real boundary, then keep named owner types.
Do not launder evidence with `unknown`, `object`, `as`, `serde_json::Value`,
or `Any`. ChenChess adapts [`dmmulroy/anti-slop`](https://github.com/dmmulroy/anti-slop)
as owned Oxlint rules in `tooling/oxlint` and as review rules for Coach
Engine Rust. Rebuild how-to: `docs/agents/typed-evidence.md`. Skill:
`typed-evidence`.

Root `.oxlintrc.json` loads `./tooling/oxlint/plugin.js` as `anti-slop`.
It also denies `eslint/complexity` (classic McCabe, pinned `max: 20`)
on the TypeScript path. `bun run lint` is the gate. Every listed rule is
`"error"`. Do not turn one off to silence a finding; fix the contract.

| Rule                                        | ChenChess note                                                                                                                                                                                                                |
| ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `no-chained-type-assertions`                | `as object as User` fabricates evidence. `as unknown as Owner` is allowed.                                                                                                                                                    |
| `no-conditional-empty-object-spread`        | Build the object in statements. Do not `...cond ? { k } : {}`.                                                                                                                                                                |
| `no-known-value-widening`                   | Prefer inference or `satisfies` over `Record<string, T>` or an inline `{ ... }` return annotation. A named alias to a closed object is an owner contract.                                                                     |
| `no-module-mocking`                         | Do not add `vi.mock` or `jest.mock`. Inject a typed seam (`authApi`, `TestFirebaseAuthProvider`, `provideReviewSessionTransport`, `removeStagingRoot`).                                                                       |
| `no-object-parameters`                      | `object` is not an owner type.                                                                                                                                                                                                |
| `no-reflect-apply` / `no-reflect-get`       | Call or read through a typed interface.                                                                                                                                                                                       |
| `no-runtime-typeof`                         | Allowed inside type predicates and named boundary parsers. Elsewhere extract a named `parse*` predicate or use a Valibot schema.                                                                                              |
| `no-shape-in-symbol-names`                  | No identifier may contain `shape`. Parse-reject names use `Form`. Visual Watercolor variants use `silhouette`.                                                                                                                |
| `no-unknown-parameters`                     | Allowed on `cause` / `error` / `err` and on `parse*` / `decode*` / `assert*` / `read*` / `from*` / type predicates. The name must be the verb or camelCase (`parseUser`), not a prefix collision (`readyState`, `assertion`). |
| `no-unknown-returns`                        | Return the parsed owner type, not `unknown`.                                                                                                                                                                                  |
| `no-unknown-type-aliases`                   | Do not hide `unknown` behind a name.                                                                                                                                                                                          |
| `no-unsafe-dictionary-type`                 | `Record<string, unknown>` is not an owner contract. Import `JsonObject` from `@chenchess/coach-engine-sdk`.                                                                                                                   |
| `no-widen-then-assert`                      | Do not widen a known value and cast it back. The one-line `as unknown as Owner` chain is the permitted rewrite.                                                                                                               |
| `require-safety-comment-for-type-assertion` | `as const` and `as unknown` are exempt. Every other assertion needs `// SAFETY:` on the previous line or statement. The outer `as Owner` in `value as unknown as Owner` still needs the comment.                              |

Effect `no-service-constructor-imports` is not copied. No direct Effect
dependency.

Prefer a Valibot schema, an inferred owner type, and named constructors.
Do not invent a field-by-field `parseIs*` walk when the public argument
list is already known. `// SAFETY:` plus `as Owner` is the last resort,
not the default rewrite.

```text
untrusted JSON/text -> as unknown -> parseName(schema, input) -> Owner
known string / mint   -> fromName(value) -> Owner
optional boundary     -> readName(input) -> Owner | undefined
```

Public Coach App and Central Host APIs are few and their call stacks are
short. Enforce the argument type at that boundary. Do not assert it again
downstream.

- New schemas use Valibot (`v.object`, `v.parse`, `v.safeParse`,
  `v.InferOutput`). Prefer Valibot over Zod. Zod remains only where a
  third-party SDK requires it (MCP `registerTool` input schemas).
- Branded Coach Engine IDs are minted and parsed in
  `@chenchess/coach-engine-sdk` (`fromGameImportId`, `mintOperationId`,
  `parseJsonObject`, …). Do not inline `as GameImportId`.
- JSON bags use the Coach Engine SDK constructors (`parseJsonObject`,
  `readJsonObject`, `jsonObjectSchema`). Do not copy a local `json-value`
  walker. `parseJsonObject` throws on non-JSON values (`function`,
  `symbol`, `bigint`, non-plain objects). `readJsonObject` returns
  `undefined`. Undefined keys are omitted as optional-field encoding.
- Optional fields are assigned in statements so TypeScript keeps
  narrowing. Do not introduce `assignedWhen` or `...cond ? { k } : {}`.
- `v.is(v.string(), value)` (or a file-local named `parse*` predicate)
  owns unknown strings. `typeof` stays inside those predicates. Do not
  wrap `typeof` in a shared helper module.
- `// SAFETY:` plus `as Owner` is last resort. Prefer a Valibot schema
  or an SDK constructor. A leftover comment on `fromX` / `mintX` is not
  an assertion and should be deleted.
- Page tests stub Firebase identity through `TestFirebaseAuthProvider`
  (React context). Production `useFirebaseAuth` reads only that context.
  Drive token and user changes through the injected `authApi` in
  `FirebaseAuthProvider` tests. Do not add a module-global override and
  do not resurrect `vi.mock`.

`parseName` may use `typeof` and type predicates under the ChenChess
`no-runtime-typeof` defaults. Callers after that parse take `Owner`, not
`unknown`. JSON object bags use imported `JsonObject` /
`{ readonly [key: string]: JsonValue }`, never `Record<string, unknown>`.

### Rust (Coach Engine)

There is no Clippy plugin that mirrors every anti-slop rule. Agents apply
the same principles when writing or reviewing `services/coach-engine`.
`scoped-validation` still runs `cargo fmt`, `cargo clippy -- -D warnings`,
and the nearest tests. Do not add new workspace-wide pedantic Clippy denies
in this change; they would fail existing numeric `as` casts.

| Anti-slop idea              | Rust reject                                                                   | Rust keep                                                                                                           |
| --------------------------- | ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| Unknown / object parameters | `fn handle(v: serde_json::Value)`, `Box<dyn Any>`                             | `fn parse(v: serde_json::Value) -> Result<Owner, _>` at the HTTP/JSON/file boundary, then `fn handle(owner: Owner)` |
| Unsafe dictionaries         | `HashMap<String, Value>` as a stored contract                                 | `HashMap<OwnerId, Owner>` or a serde struct                                                                         |
| Chained assertions          | `value as u64 as usize` after a JSON number, or transmute through `*const ()` | `TryFrom` / `from` at the boundary; trivial numeric `as` when the math already bounds the width                     |
| Widen then assert           | Encode as `Value`, then `as_object().unwrap()` back to a known shape          | Keep the struct                                                                                                     |
| Runtime typeof              | `if value.is::<T>()` / `TypeId` dispatch for domain data                      | Serde / enums                                                                                                       |
| Module mocking              | Replacing a module with `mockall` to avoid a trait                            | The existing test helper / trait seam in that crate                                                                 |
| SAFETY comments             | Bare `unsafe` or an `as` that changes meaning                                 | `// SAFETY: <invariant>` on the previous line                                                                       |

`serde_json::Value` stays legitimate in:

- transport tests that must accept unshaped JSON;
- evaluation or telemetry snapshots whose schema is the JSON document;
- a parser's first local binding before `serde_json::from_value`.

It is not a Coach Engine or Review Engine domain type.

Do not:

- Thread `Value` or `Box<dyn Any>` through Review Engine or Coach Engine
  internals.
- Add `#[allow(clippy::…)]` to silence a missing type. Fix the contract.
- Mock a module to avoid a seam. Follow nearby test helpers.

## Internal contracts

Validate untrusted data once, convert it to a trusted representation, then
trust the language's enforced internal contracts. See
`defensive-code-cleaner`.

Before adding or keeping a defense, answer: what concrete runtime path can
produce the invalid state this handles? If no reachable path exists, do not
add it.

Do not add guards for states the compiler already excludes. Do not spread
uncertainty downstream through optional parameters, repeated guards, or
fallback defaults.

## Design system

Astryx on StyleX is the ChenChess styling foundation, and the **watercolor
brand layer rides on top of it**. Product chrome uses the `Watercolor*`
primitives from `@chenchess/ui/components/watercolor` — buttons, cards,
badges, chips, notices, plaques, chat bubbles, form controls, studio pages.
Never add an eyebrow, kicker, or subtitle on any surface — cards, page
sections, and heroes alike — unless the user asked for that exact copy
(`docs/design/product-chrome.md`). Player-visible success and
failure use `WatercolorNotice` / `AuthNotice`, never Astryx `Banner`.
Their craft is authored StyleX in `watercolor.styles.ts`; the
`chen-watercolor-*` classes are structural hooks only. Raw Astryx components
are for neutral scaffolding: layout stacks, previews, backoffice, and the
foundation check. Never strip the watercolor layer to "adopt Astryx fully" —
that was regression #465; the visual target is
`docs/design/brand/chenchess-workspace-application-target.jpg`. No leftover
Tailwind, shadcn, Base UI, or external icon libraries (icons go through
`Icon` from `@chenchess/ui/astryx`, registry in `packages/ui/src/icons.tsx`)
on any surface you touch. Author new styling as StyleX (`stylex.create` +
`stylex.props`/`xstyle`), never fresh CSS files or rules — plain CSS remains
only where React cannot reach, and only on the closed list below.
Colour values live in `theme/inkWash.ts` and `chenTokens.css`; the craft names
tokens, so a wash is `color-mix(in srgb, var(--color-ink) 9%, transparent)`
rather than `rgb(20 43 70 / 0.09)`. The per-surface layout stylesheets
are gone; there is no category left for a new one. The landing page renders from React and
Astro SSRs it into `index.html` at build time (`astro.config.ts`) so it still
reads and paints without JavaScript — `verify:public-build` fails if that
markup goes missing. In Vitest graphs `@stylexjs/stylex` is aliased to
`packages/ui/src/vitest.stylexShim.ts` — jsdom never verifies visuals; the
browser gates do. Visual verification runs through Storybook
(`bun run --cwd apps/central-host storybook`): component stories live in
`packages/ui/stories/`, page stories with fixture data in
`apps/central-host/stories/`. Storybook is a local workflow and is never built
or served by the deployed host — its fixtures are MSW service workers and a
real Auth emulator, neither of which belongs on a hosted origin. Authed page stories run the real
clients against real doubles — Firebase Auth through `connectAuthEmulator`
(`bun run --cwd apps/central-host emulator:auth`) and Coach Engine REST and
the Review Session ndjson command stream through MSW handlers, the stream
answered by the App test's own fixture
(`apps/central-host/src/review-session/reviewSessionStreamFixtures.ts`); never
hand-write a context double for them.

The `<!-- ASTRYX:START -->` block is generated by
`astryx init --features agents` and refreshed by `astryx upgrade`. It is the
discover-first workflow, the no-`<div>` rule, and the StyleX `xstyle` rule.
These lines are ChenChess-specific and sit outside the markers so an upgrade
does not overwrite them. After `astryx upgrade`, if it injects a new
`<!-- ASTRYX:START -->` block into `AGENTS.md`, move that block here.

- **Tokens.** Write token values only in `packages/ui/src/theme/inkWash.ts`.
  Run `bun run --cwd packages/ui theme:build`. The only extra tokens are the
  board palette and `--font-family-seal` in
  `packages/ui/src/theme/chenTokens.css`.
- **StyleX compiler.** Required. Every Vite, Storybook, or Vitest graph that
  compiles ChenChess UI must run `chenStylexVitePlugin()` from
  `@chenchess/ui/stylex.vite` _before_ the React plugin. Author
  `stylex.create` and pass the result as `xstyle`. Typed tokens come from
  `@astryxdesign/core/theme/tokens.stylex`.
- **Layout.** No authored `<div>`, `<span>`, `<p>`, or `<h1>`–`<h6>` for
  structure or copy. Use `AppShell`, `Layout`, `VStack`, `HStack`, `Grid`,
  `Heading`, and `Text`. Astryx may render a `div` internally; you do not.
- **CLI.** `bunx astryx <cmd>` from the repo root, or
  `bun run --cwd packages/ui astryx -- <cmd>`.
- **CSS entry.** The generated SETUP block still names `reset.css` /
  `astryx.css`. Do not follow that. The override sits immediately after
  `<!-- ASTRYX:END -->`.

### Permanent CSS

A stylesheet not on this list is a defect, and adding one means adding its
row here with the reason in the same change.

Counted with:

```
find packages apps -name "*.css" -not -path "*/node_modules/*" -not -path "*/dist*"
```

| File | Lines | Why it cannot be StyleX |
| ---- | ----- | ----------------------- |
| `packages/ui/src/theme/generated/inkWash.css` | 671 | generated by `astryx theme build` from `theme/inkWash.ts` |
| `packages/ui/src/workspace/WatercolorBoard.css` | 225 | board internals — nth-child square skins, piece sprite offsets, and the board primitives the web reads (#430) |
| `packages/ui/src/workspace/review-session.css` | 207 | descendant craft reaching into markup another component owns, plus the Review Session column scroll hooks StyleX cannot attach to those descendants |
| `packages/ui/src/theme/chenTokens.css` | 121 | the ChenChess tokens Astryx's `tokens` field cannot express |
| `packages/ui/src/styles/base.css` | 112 | element-level defaults under the `chen-base` layer |
| `packages/ui/src/styles/globals.css` | 78 | document defaults and the import chain |
| `packages/ui/src/theme/surfaces.css` | 24 | the `surfaces` layer seam that `PublicLayout.astro` imports |
| `packages/ui/src/theme/generated/watercolorShapes.css` | 17 | generated clip-path and mask shapes from the brush scans |
| `packages/ui/src/styles/layers.css` | 13 | cascade layer order |
| `packages/ui/src/theme/theme.css` | 13 | the theme import seam |
| `packages/ui/src/theme/foundation.css` | 8 | the foundation import seam |
| `apps/central-host/src/styles.css` | 6 | the `#root` mount node |

12 rows, 1495 lines. The snapshot is concatenation, not a third hand copy
of the primitives.

`globals.css` lost 520 lines in #430: every rule it still held for the
pre-revamp web app, whose surfaces the #418 children rebuilt in StyleX. Nothing
mounted those classes any more.

### Styling consistency

#430 swept these across every surface. They are review rules, not gates — with
one exception noted below, no script enforces them, so a reviewer has to. Each
has exceptions that are the right answer, and a check that mechanically
rejected them would be wrong.

- **Name a token, never a colour.** Values live in `theme/inkWash.ts` and
  `chenTokens.css`; everywhere else reads `var(--…)`. Real exceptions: a
  pre-hydration shell whose `var(--token, #fallback)` is what a Player sees for
  the first frame; a mask, where `#000`/`#fff` are alpha stops rather than
  brand colours; and a block that *remaps* a token, where naming it would read
  the remapped value back — the dark dialog in `watercolor.styles.ts` explains
  that trap in place. Do not leave a `var(--token, #fallback)` pair anywhere
  the token is guaranteed present; the fallback is dead and drifts.
- **Name a foundation breakpoint.** The five cuts in `theme/breakpoints.ts`
  are the set, plus the `min-width: 64.01rem` complement that undoes the
  `64rem` stack cut (`review-session.css`) — a partner to a cut, not a sixth
  cut. `theme/breakpoints.ts` cannot be imported into a `stylex.create` that
  the Coach App artifact build compiles, so every StyleX file copies the
  literal into a local `const` instead. Copying a *different* width is the
  defect, and it is the one half of this rule a script can judge, so it does:
  `tooling/scripts/foundation-breakpoints.test.ts` reads the widths back out of
  `theme/breakpoints.ts` and holds every copy in `apps/*` and `packages/*` to
  them. Whether the local is *named* after the cut it copies stays a review
  rule. Name the local after the foundation cut it copies: today `compact`
  means 860px in `authStudio.styles.ts` while `narrow` means 520px in
  `ReviewNavigation.styles.ts`, which reads as two different scales.
- **Spacing is Astryx's 4px grid** (`spacingVars`: 0/2/4/6/8/12/16/20/24/28/
  32/36/40/44/48px). `clamp()` is still the right answer for fluid space, the
  grid stops at 48px and does not describe anything larger, and a negative
  length is not a spacing step — `margin: -1px` is the screen-reader clipping
  idiom.
- **One owner per class.** A primitive gets its rules in one stylesheet. Where
  two files legitimately style the same hook, the difference has to be the
  point: `WatercolorBoard.css` paints the board, and `review-session.css`
  attaches column scroll hooks StyleX cannot reach.
- **Delete what a redesign orphans.** A rule whose class no markup mounts, and
  every rule scoped under it, goes with the surface it belonged to. Check by
  whole token and remember runtime-built names: `` `chen-review-moment-${tone}` ``
  never appears whole in source, `chen-brand-lockup` contains `chen-brand`, and
  Astryx renders its own `.astryx-*` classes.

Colour values live in `packages/ui/src/theme/inkWash.ts` (the Astryx theme) and
`chenTokens.css` (the names Astryx cannot express — board palette, review
moment tones, depth stops, the brand aliases the craft reads by). Everywhere
else names a token: a wash is
`color-mix(in srgb, var(--color-ink) 9%, transparent)`, never
`rgb(20 43 70 / 0.09)`, so a retheme reaches the tints and not just the solid
fills.

The one exception is painting before the theme arrives: a stylesheet that runs
outside the token chain may keep brand hexes as `var(--token, #hex)` fallbacks.

<!-- ASTRYX:START -->

Astryx v0.4.2 · 156 components
CLI: run every command as `bunx astryx <cmd>` (shown below as `astryx ...`).

SETUP (once, in your app entry e.g. main.tsx) — without these, components render unstyled:
import "@astryxdesign/core/reset.css";
import "@astryxdesign/core/astryx.css";

WORKFLOW — discover, don't guess. Before writing UI:

1. `astryx build "<idea>"` — START HERE: returns a kit (closest [page] + [block]s + [component]s). No args = full playbook.
2. `astryx template <name> [--skeleton]` — scaffold the [page]/[block]s it named, or study their layout. Templates are reference code.
3. `astryx component <Name>` — props + examples for every component you use.

RULES:

- No <div> — components do all layout/spacing, page frame included.
- Frame first: read `astryx docs layout` before writing any page or screen — page frame, region widths, breakpoint behavior.
- Dense data = rows (Table, List/Item), never Card-wrapped list items; Card is for standalone widgets. Status = StatusDot/Token; Badge = counts only.
- Custom styling: component props first; else the xstyle prop / StyleX tokens (@astryxdesign/core/theme/tokens.stylex). No raw hex/px.
- Tokens for every value (`astryx docs tokens`). Brand/accent belongs in the theme (`astryx theme list` / `theme add <slug>`, or `astryx theme template` for a custom one) — never override --color-* in :root.
- SELF-CHECK before you finish: re-read the file and replace any className=, style={{…}}, raw <div>/<span> layout, imported .css/@apply, or hardcoded #hex/px with the component or the xstyle prop + a token. If unsure a component/prop exists, run `astryx component <Name>` / `astryx search "<thing>"`; don't hand-roll CSS.

MORE CLI:
search "<query>" find any component / hook / doc / template / block
component --list 156 components by category
template --list page + block recipes
docs <topic> color, elevation, icons, illustrations, internationalization, layout, migration, motion, principles, shape, spacing, styling, theme, tokens, typography
swizzle <Name> eject component source for deep customization
upgrade --apply run after any @astryxdesign/core bump
<!-- ASTRYX:END -->

ChenChess CSS entry (overrides SETUP above): never import
`@astryxdesign/core/reset.css` or `astryx.css` from an app entry. Import
`@chenchess/ui/styles.css` or `@chenchess/ui/theme.css`, which load
`packages/ui/src/theme/foundation.css` — `layers.css` first, then the Astryx
sheets, then the built ink-wash theme. An unlayered Astryx import recreates
the closed-dialog trap.

## Pointers

- Domain vocabulary: `CONTEXT.md`
- Agent rules: `AGENTS.md`
- Product chrome copy: `docs/design/product-chrome.md`
- Typed-evidence rebuild: `docs/agents/typed-evidence.md`
- Typed evidence: `docs/agents/typed-evidence.md`
  Tautological tests considered harmful.
