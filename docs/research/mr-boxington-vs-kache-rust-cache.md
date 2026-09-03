# mr-boxington (mbx) as a replacement for Kache

Research date: 2026-08-29. Head-to-head benchmark: 2026-08-30.
Upstream versions assessed:
[`jdx/mr-boxington` v1.0.0](https://github.com/jdx/mr-boxington/releases/tag/v1.0.0)
(released 2026-08-29) against the deployed
[`kunobi-ninja/kache` v0.12.0](https://github.com/kunobi-ninja/kache/releases/tag/v0.12.0).

Evidence labels: **[measured]** run on this machine against this checkout, with
the numbers below; **[repo]** verified in this checkout; **[verified]**
confirmed from upstream primary sources (GitHub API, repository source, raw
docs); **[vendor]** upstream documentation claim, not independently measured;
**[inference]** reasoned, unmeasured.

## Recommendation

On the measurements, **mbx in prefix mode is better than the current Kache
setup** — 15% faster on the cross-checkout case and free of the read-only
artifact defect. The reason to still hold is not performance, it is that the
tool is ten days old with **no public issue tracker**.

Two results carry the verdict **[measured]**:

- Kache's cross-checkout cache buys essentially **nothing in wall time here**:
  74.57s warm against 74.72s cold, despite reporting a 41.9% hit rate. mbx
  prefix mode goes 77.09s cold to 63.60s warm.
- The read-only restored-artifact defect **reproduced under Kache and not
  under mbx**: 312 of 376 `.rmeta`/`.rlib` files lacked the owner-write bit
  after a Kache cross-checkout restore; mbx produced 0 of 376.

Against a fair no-cache reference of 79.40s, Kache saves ~6% and mbx saves
~20%. Both are modest, because the repo's own heaviest crate
(`chen_chess_coach_engine`, 77–86s) never caches under either tool.

So the trade is: roughly 11 percentage points of build time and the retirement
of the `chmod u+w` repair, bought with ~13 call-site rewrites, a hand-rolled
Nix pin, and a dependency on a project where
[GitHub Issues are disabled](https://github.com/jdx/mr-boxington)
(`has_issues: false`) so there is no bug history to audit and no way to report
a correctness failure **[verified]**. This repo scoped its own Kache risk by
reading kache issues #330, #431, and #635; that method is unavailable here.
Re-evaluate in roughly a quarter, or sooner if upstream opens issues.

## Head-to-head benchmark

Host: macOS 26.6.2, arm64, 10 CPUs, 32 GiB RAM. Toolchain cargo/rustc 1.97.1.
Source revision `7442c3423a8d1261f3f8fc0775b3865e4b5d5992`. mbx 1.0.0 from the
checksum-verified `mbx-aarch64-apple-darwin.tar.gz` release archive. Two
disposable jj workspaces at that revision. **Every arm got its own empty
`CARGO_TARGET_DIR`**, which also neutralises mbx's managed-target symlinking,
so both tools ran on the same footing **[measured]**. Kache used a scratch
`KACHE_CACHE_DIR`, leaving the developer store untouched. Arms ran
sequentially. The command is the compile-only one from the Kache pilot guide
(`docs/kache-pilot.md`, removed by the migration this note argued for; its
successor is `docs/rust-build-cache.md`):

```sh
cargo test --workspace --lib \
  --test domain --test session --test boundary --test runtime --no-run
```

| Arm | Wall | vs vanilla | Cache result | Read-only rmeta/rlib |
| --- | --- | --- | --- | --- |
| Vanilla, no cache (first run, cold page cache) | 85.75s | — | — | 0/379 |
| **Vanilla, no cache (control, warm page cache)** | **79.40s** | **reference** | — | 0/376 |
| Kache cold, empty store | 74.72s | −5.9% | 0 hits, 204 misses | 0/376 |
| Kache warm, cross-checkout | 74.57s | −6.1% | 41.9% hits, 28.4% weighted | **312/376** |
| mbx cold, empty store | 77.09s | −2.9% | 0 hits, 71 bypassed | 0/376 |
| **mbx warm, cross-checkout** | **63.60s** | **−19.9%** | 165 hits, 25 misses, 71 bypassed | **0/376** |

Ordering matters: the first arm paid cold OS page-cache costs, so vanilla was
re-run last as a control and came in 6.35s faster. The control is the honest
reference. **mbx warm is 14.7% faster than Kache warm** (63.60s vs 74.57s).

Store cost was comparable — Kache 1.6 GiB, mbx 1.4 GiB — once managed targets
were disabled. With managed targets on, an earlier run showed mbx holding
13.8 GiB of target directories for two checkouts on top of the action store
**[measured]**.

mbx's warm run reports 84.10s of compiler time avoided and materialised 328
outputs (451.2 MiB) by reflink plus 165 (916.9 KiB) by copy in 229.2ms.
Bypasses were identical cold and warm: 71 compilations — 57
`unportable-native-link`, 6 `standard-input`, 5 `compiler-query`, 3
`native-library`. The advertised macOS native-link caching does not materialise
for this workspace's binaries, which is the same practical position as Kache's
`KACHE_CACHE_EXECUTABLES=0` **[measured]**.

## The read-only rmeta defect, reproduced head to head

Kache restores are APFS clones that inherit the cache entry's read-only mode,
so a later wrapper-free `cargo` can fail with `output file ... is not
writeable`; `sweep-targets.ts` carries a `chmod u+w` repair for it **[repo]**.

The benchmark reproduced the precondition exactly. After the Kache
cross-checkout restore, **312 of 376** `.rmeta`/`.rlib` files lacked the
owner-write bit. After the mbx cross-checkout restore — 493 outputs actually
materialised — **0 of 376** did **[measured]**. The predicate was the one
`readOnlyArtifacts` used in `tooling/scripts/sweep-targets.ts` — that repair
existed only for the Kache defect and was removed with Kache itself, so the
function no longer exists; the equivalent check is
`find -L target -type f ! -perm -u+w`.

A plain wrapper-free `cargo` run over each restored target exited 0 in both
cases, because Cargo considered the restored artifacts up to date and never
had to overwrite them. The read-only mode is the loaded gun, not the discharge;
it bites when a later build must rewrite one of those files. Under mbx the
repair path would be dead code. Verified for these versions on APFS; not a
proof for the copy fallback or other filesystems.

## The developer Kache store is in worse shape than the benchmark

The controlled Kache arm reached 28.4% weighted savings. The actual
developer store does considerably worse **[measured]**:

```
Store:      50.0 GiB / 50.0 GiB (1025 entries, 100%)
Dedup:      2308 unique blobs, 49.9 GiB physical, 0.1% savings
Hit rate:   51.5% (local: 88, prefetch: 0, remote: 0, dup: 14, miss: 69)
Weighted:   14.8% by compile cost
Miss share: 96.0% of wrapper time (~9min)
Time saved: ~1min (estimated compile work avoided, last 24h)
```

It is pegged at its 50 GiB cap, dedup is returning 0.1%, and 96% of wrapper
time is misses. Whatever is decided about mbx, the incumbent is consuming
50 GiB to save about a minute a day, which is worth investigating on its own
**[inference]**.

## mbx does have an ambient mode — and it is much weaker

The first pass of this note claimed there is no plain-`RUSTC_WRAPPER` mode.
That was wrong, and it was the assumption behind the "rewrite every call site"
objection.

`mbx setup` is a **hidden but functional** subcommand
([crates/mbx/src/cli.rs](https://github.com/jdx/mr-boxington/blob/main/crates/mbx/src/cli.rs),
`#[usage(hide)]`) that installs a persistent rustc shim and writes
`build.rustc-wrapper` into Cargo's config — structurally identical to how
`flake.nix` sets `RUSTC_WRAPPER` for Kache today. Install and uninstall
round-trip cleanly and `mbx doctor` reports the state **[measured]**.

Upstream's own source comment states the trade:

> Hidden, not removed: `mbx <cargo command>` is the supported path and gets the
> remote cache, statistics, managed targets, and collection that the standalone
> wrapper cannot, but setups already relying on this keep working.

Scored against this repo the listed losses are mostly tolerable — the remote
cache is irrelevant under `local_only`, managed targets collide with
`sweep:targets` anyway, and `mbx gc` still works standalone **[measured]**. The
disqualifying cost is the one upstream does not mention: measured against a
store warmed by two prior builds, the ambient shim took **89.47s** where prefix
invocation took 68.40s on a *colder* store — roughly a third of the benefit,
and it prints no statistics at all **[measured]**.

So the real choice is between two imperfect options:

1. **Ambient (`mbx setup`)** — a genuine drop-in for the current wiring, no
   call-site edits, agents' ad-hoc `cargo` stays cached — but most of the
   speedup evaporates and it leans on a deliberately hidden code path.
2. **Prefix (`mbx build`)** — the full ~20% benefit, but every cargo call site
   must change and any unprefixed call goes silently uncached.

## Migration surface if adopted in prefix mode

Roughly 13 mechanical edits, mostly the first element of a JSON command array
**[repo]**:

- Six `chenchess-rust#*` turbo tasks ([turbo.json](../../turbo.json)): `format`,
  `lint`, `test`, `review-session-contract-drift`,
  `review-session-recording-integrity`, `deterministic-evaluation`.
- Two root scripts ([package.json](../../package.json)): `dev:coach-engine`,
  `gotham`.
- Five `tooling/scripts` call sites: `daily-coaching-conformance.ts`,
  `release-proof-plan.ts`, `release-targets.ts` (×2), `release-proof.ts` — all
  release-path, so these should stay on plain `cargo`.

The uncovered surface is the problem: **agents' ad-hoc `cargo` commands** are
cached today because Kache is ambient, and would silently stop being cached
**[inference]**.

Other coupling:

- **Nix wiring gets worse.** No upstream flake and no nixpkgs package, so the
  flake must fetch a release binary or build the crate **[verified]**. In
  exchange the `.#kache`/`.#vanilla` split and `KACHE_*` env pinning could
  collapse, and the `RUSTC_WRAPPER` rejection in
  [release-process.ts](../../tooling/scripts/release-process.ts) stays valid
  under prefix mode **[inference]**.
- **`sweep:targets` vs managed targets.** mbx symlinks `target/` into its cache
  root unless `CARGO_TARGET_DIR` is set or `target.views = false` **[measured]**.
  Pick one owner; running both double-manages the same directories.
- **Doc pins.** `README.md`, `docs/kache-pilot.md` (since removed),
  `docs/local-ci.md`,
  `.cursor/skills/scoped-validation/SKILL.md`, `flake.nix`, `.kache.toml`,
  `tooling/nix-develop`, and three `tooling/scripts` test files **[repo]**.
- **No coexistence.** Both claim `RUSTC_WRAPPER`; mbx defers and caches nothing
  when it is already set, so any trial runs from `.#vanilla` **[vendor]**.

## What is still not addressed

The environment-sensitive proc-macro stale-hit class that keeps Kache out of
release gates. A `RUSTC_WRAPPER` shim cannot observe an env var a proc macro
reads at expansion time, and mbx documents no strict full-environment mode, so
the vanilla-release boundary must survive any migration **[inference]**.

One accuracy caveat on upstream's comparison page: it states "kache hardlinks
outputs into place", but this repo's own assessment established that Kache
reflinks on APFS and hardlinks only as a non-CoW fallback
([kache research](kache-cross-worktree-rust-cache.md)) **[repo]**.

## Reproducing this benchmark

```sh
# Two disposable workspaces at one immutable revision
jj workspace add --name bench-a ../chenchess-bench-a --revision <rev>
jj workspace add --name bench-b ../chenchess-bench-b --revision <rev>

# mbx 1.0.0, checksum-verified, from the release archive
curl -sSL -o mbx.tar.gz \
  https://github.com/jdx/mr-boxington/releases/download/v1.0.0/mbx-aarch64-apple-darwin.tar.gz

# Every arm: its own empty CARGO_TARGET_DIR, scratch store, sequential.
# Kache uses a scratch store so the developer store is untouched:
KACHE_CACHE_DIR=/tmp/bench/kache-store ./tooling/nix-develop .#kache --command cargo <args>
# mbx runs from the wrapper-free shell, since it defers to a set RUSTC_WRAPPER:
./tooling/nix-develop .#vanilla --command <path>/mbx <args>

# Read-only artifact check (the Kache failure predicate)
find -L "$CARGO_TARGET_DIR" -type f \( -name '*.rmeta' -o -name '*.rlib' \) ! -perm -u+w | wc -l

# Re-run the no-cache arm LAST as an ordering control; page-cache warmth is
# worth ~6s on this workload.
```

## Open questions

1. Why does Kache's 41.9% hit rate / 28.4% weighted savings produce no
   wall-clock improvement at all here (74.57s warm vs 74.72s cold)?
2. Can the 57 `unportable-native-link` bypasses be reduced, and would that make
   `chen_chess_coach_engine` — the crate that dominates every arm — cacheable
   under either tool? `mbx explain` would name each one.
3. Why is the ambient shim roughly three times weaker than prefix invocation?
   Upstream has no issue tracker to ask on.
4. Is the developer Kache store's 0.1% dedup and 14.8% weighted savings a
   misconfiguration worth fixing independently of this decision?
5. If mbx is adopted in prefix mode, what keeps agents' ad-hoc `cargo` calls
   from silently going uncached?
