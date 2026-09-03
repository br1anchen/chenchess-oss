# Rust build cache (mbx)

ChenChess compiles Rust through [mbx](https://mr-boxington.jdx.dev/), a
content-addressed build cache shared by every checkout on the machine. The
measurements that chose it over the previous cache are in
`docs/research/mr-boxington-vs-kache-rust-cache.md`.

## Why there is a seam and not a wrapper

mbx caches only what is invoked as `mbx <cargo command>`, and it *defers* to an
already-set `RUSTC_WRAPPER`. Exporting a wrapper from the dev shell — the
obvious way to wire a compiler cache, and how the previous one worked — would
therefore cache nothing, silently, while every build still passed.

So no shell exports a wrapper, and cached call sites go through one seam:

```
tooling/cargo-cached <cargo args>
```

`tooling/cargo-cached` runs `mbx` when mbx is on `PATH` and plain `cargo`
otherwise, announcing the uncached run on stderr. That is what lets the same
`turbo.json` work on a laptop, in a Cloud Agent image, and in a Claude cloud
session that may not carry mbx at all.

A bare `cargo build` still works. It is simply uncached — which is the trap,
because an uncached build is indistinguishable from a cached one except by its
wall time. So the rule for people and agents alike is: **build Rust with
`./tooling/cargo-cached`, never a bare `cargo`**. `AGENTS.md` and the
`scoped-validation` skill carry it too.

Gates need no discipline here: every turbo task and root script already routes
through the seam, so `bun run test --filter=chenchess-rust` is cached whoever
runs it. Only hand-typed commands can miss.

## Shells

| Command | Carries mbx | Use for |
| --- | --- | --- |
| `./tooling/nix-develop` | yes | ordinary implementation work |
| `./tooling/nix-develop .#mbx` | yes | explicit alias for the default |
| `./tooling/nix-develop .#vanilla` | **no** | release gates, proofs, deployments, uncached comparison |

## Release work is cache-free, by assertion

Release steps reach this seam indirectly: `railway-coach-engine-verification`
runs `turbo run lint test --filter=chenchess-rust`, and those tasks call
`tooling/cargo-cached`. So keeping mbx off `PATH` is not on its own a boundary —
it holds on a laptop in `.#vanilla` and fails on a Cloud Agent image, which
carries mbx in `/usr/local/bin` and has no Nix.

The boundary is therefore explicit. A build that must not read a cache marks
every process it spawns with `CHENCHESS_UNCACHED_CARGO=1`, and the seam runs
plain Cargo whenever it is set, whatever is on `PATH`. A test pins that
behaviour by running the seam against fake `mbx` and `cargo` binaries and
asserting which one
executed.

Three independent things therefore have to fail before a release reads a cache:
the marker, `.#vanilla` withholding mbx, and each release entry point's
rejection of a non-empty `RUSTC_WRAPPER` / `RUSTC_WORKSPACE_WRAPPER`.

## Installation, per environment

mbx publishes no nixpkgs package and no flake, so version **1.0.1** is pinned in
`flake.nix` by per-platform release digest (`aarch64-apple-darwin` on macOS,
static musl on Linux). Only the two systems this repository is developed on are
pinned; elsewhere `mbxSupported` is false and the default shell falls back to
the uncached vanilla shell.

An environment without mbx still runs every check — uncached, and
`cargo-cached` says so on every invocation.

## Target directories

mbx would normally replace `target/` with a symlink into its own store
("managed targets"). This repository turns that off with `MBX_TARGET_VIEWS=0`.

`tooling/cargo-cached` sets it for every build it runs, which is what makes it
hold wherever mbx arrives without an environment file to carry it. The Nix shell
sets it too, so an ad-hoc `mbx` typed outside the seam cannot take `target/`
over either.

`cargo-sweep`, via `bun run sweep:targets`, stays the single owner of `target/`
cleanup. `mbx gc` collects only mbx's own action store. Do not run both against
the same directory.

## Worktrees and workspaces

mbx keys its store by content, and the store is shared machine-wide, so a second
Git worktree or Jujutsu workspace at the same revision restores from the first
one's build instead of recompiling. That is the main reason the cache exists
here. The measured cross-checkout run was 63.60s against 79.40s uncached.

Jujutsu note: mbx identifies a colocated repository through Git. Its
jj-native discovery covers `mbx exec` only, which this repository does not use.

## What mbx does not cache

Roughly 71 compilations bypass on this workspace, dominated by
`unportable-native-link` — so binaries and test binaries relink every time, and
`chen_chess_coach_engine` itself is never restored. `mbx explain <cargo command>`
names every bypass. Incremental compilation is bypassed by default; the sweep's
`--purge-incremental` therefore costs a genuine rebuild.

## Rollback

Revert the commit. It touches `flake.nix`, `flake.lock`, `tooling/cargo-cached`,
`tooling/nix-develop`, `tooling/scripts/sweep-targets.ts`, `turbo.json`,
`package.json`, `README.md`, and the `tooling/scripts` test files. Nothing in
the build outputs or the repository's
source depends on which cache produced them, so a revert needs no target
cleanup.

## Upgrading mbx

1. Read upstream's release notes. The project ships fast — 1.0.0 and 1.0.1
   landed a day apart — and has **no public issue tracker**, so a regression
   surfaces here first.
2. Update the version and all three digests in `flake.nix`, the URL and checksum
   in `flake.nix`.
3. Re-run the head-to-head protocol in
   `docs/research/mr-boxington-vs-kache-rust-cache.md#reproducing-this-benchmark`
   — two disposable workspaces, explicit empty `CARGO_TARGET_DIR` per arm, and
   the no-cache arm run last as an ordering control.
