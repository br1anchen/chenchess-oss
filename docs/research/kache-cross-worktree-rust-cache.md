# Kache for Rust reuse across Git worktrees and Jujutsu workspaces

Research date: 2026-07-29  
Upstream version assessed: [`kunobi-ninja/kache` v0.12.0](https://github.com/kunobi-ninja/kache/releases/tag/v0.12.0)

> **Decision update (2026-08-03):** Issue
> #183 superseded the
> rollout recommendation below. Kache is now the default local development
> shell; the wrapper-free `.#vanilla` shell remains mandatory for release work.
> The assessment below is retained as the evidence and risk analysis behind
> that boundary.

## Recommendation

Evaluate Kache in a named, opt-in, local-only Nix development shell, with one
target directory per Git worktree or Jujutsu workspace. Keep the default shell
and all release gates uncached. Do not share `CARGO_TARGET_DIR` between
workspaces. On this Apple Silicon/APFS host, Kache should normally restore
cached artifacts with APFS copy-on-write clones (`clonefile`), not hardlinks.
That provides shared physical blocks without giving separate workspaces the
same mutable inode. Kache only falls back to hardlinks for artifacts it
classifies as immutable when reflinks are unavailable; mutable or OS-loaded
outputs fall back to copies.

Do not make it the default merely because a two-workspace ChenChess benchmark
passes. Kache's newest stable release is v0.12.0, published 2026-07-28, and a
new upstream report demonstrates a semantic stale hit when a proc macro
branches on an environment variable that rustc/Cargo does not track. The
reported failure reproduced three times and can, by the reporter's analysis,
silently produce a wrong binary rather than only a compiler error. Restore
checksums do not detect that class: the wrong cached blob still matches its
content address and key.
[v0.12.0 release](https://github.com/kunobi-ninja/kache/releases/tag/v0.12.0),
[issue #635](https://github.com/kunobi-ninja/kache/issues/635)

Kache's own issue tracker also documents a cross-checkout hit-rate limitation:
paths embedded inside `.rmeta` can make otherwise identical artifacts differ
and cascade misses through dependants. One upstream Substrate measurement
reported 62.7% warm hits, 74.2% key stability, and only 1.16x speedup. The
problem may be much smaller for ChenChess, but upstream evidence does not
justify assuming that.

Keep macOS executable caching off during the pilot. It is off by default
because cached Mach-O executables can retain `N_OSO` debug references to object
files in the checkout that produced them. Consequently, expect reuse for
libraries, metadata, proc macros, and dynamic libraries, but not the final
`chenchess`, server, or test executable link. Also measure normal same-workspace
edit/rebuild cycles: Kache strips Cargo's incremental-compilation flag, so a
cross-workspace win can coexist with a slower tight edit loop.

Sources:

- Kache's restore policy and artifact mutability split:
  [architecture](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/how-it-works/architecture.mdx#the-local-store),
  [deduplication](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/deduplication.mdx#zero-copy-restores),
  and [`link.rs`](https://github.com/kunobi-ninja/kache/blob/v0.12.0/src/link.rs#L63-L81).
- The cross-clone `.rmeta` gap:
  [issue #330](https://github.com/kunobi-ninja/kache/issues/330) and
  [issue #431](https://github.com/kunobi-ninja/kache/issues/431).
- macOS executable-debug limitation:
  [configuration reference](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/getting-started/configuration.mdx#all-settings)
  and [issue #319](https://github.com/kunobi-ninja/kache/issues/319).
- Incremental compilation behavior:
  [quick start](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/getting-started/quick-start.mdx#what-kache-does-not-cache).
- Environment-sensitive proc-macro stale-hit risk:
  [issue #635](https://github.com/kunobi-ninja/kache/issues/635).

## What Kache would share

Kache is a `RUSTC_WRAPPER`, not a shared Cargo target directory. Cargo keeps
writing each workspace's own fingerprints, build-script outputs, and final
artifacts under that workspace's `target/`. For each rustc invocation, Kache
computes a content-derived key, looks in a user-level store, and materializes a
hit into that invocation's output directory. This is independent of whether
the checkout was made by Git or Jujutsu; the relevant distinction is the
compiler inputs and checkout path, not the SCM command that created it.
[Wrapper flow](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/how-it-works/architecture.mdx#the-wrapper)

The key includes rustc and linker identity, source content, dependency
artifacts, compilation flags, target, feature/cfg values, compile-time
environment values, and the active emit set. Checkout, Cargo home, Rustup home,
temporary, and target paths are remapped to stable sentinels by default. That
normalization is what is intended to let identical work in distinct absolute
paths hit the same entry.
[Cache-key inputs](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/how-it-works/cache-key.mdx#whats-in-the-key)

`KACHE_RUSTC_PATH_NORMALIZE=0` is therefore not suitable for the desired
cross-workspace setup. It restores literal local paths to debug information,
but also folds the real checkout identity into the key, intentionally
partitioning artifacts by checkout. On macOS, keeping normalization enabled
means LLDB needs a source map from `/kache/workspace` to the active checkout.
[Debugging and the path-normalization opt-out](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/how-it-works/cache-key.mdx#debugging-cached-binaries)

No shared daemon is required for local reuse. The wrapper and local store work
when the daemon is absent; the daemon owns remote checks, uploads, and
prefetching. A local-only first phase can therefore avoid a new login service
and all network behavior.
[Daemon behavior](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/how-it-works/architecture.mdx#the-daemon)

## ChenChess fit

The repository has one Cargo workspace member and pins Rust 1.95.0, exactly
Kache v0.12.0's minimum Rust version. The Nix development shell already owns
tool availability, while Turbo invokes Cargo for formatting, clippy, the
curated test suite, contract drift, recording integrity, and deterministic
evaluation. A pinned Kache package and `RUSTC_WRAPPER` in a separate opt-in Nix
shell is therefore a narrower integration seam than `kache init`, which edits
the user-global Cargo configuration and installs a login daemon.
[workspace](../../Cargo.toml),
[toolchain](../../rust-toolchain.toml),
[development shell](../../flake.nix),
[Rust Turbo tasks](../../turbo.json),
[Kache's Rust requirement](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/getting-started/installation.mdx#installation),
[Kache init behavior](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/getting-started/quick-start.mdx#quick-start)

The upstream flake has no Kache binary cache. Its own issue says a Nix consumer
must build the Kache derivation, rust-overlay toolchain, and project-specific
closure locally rather than substitute them from an upstream cache. On this
host that is a one-time shared Nix-store setup cost, not one build per
worktree, but it must be measured and documented. Do not mistake the initial
`nix develop .#kache` build for ChenChess cache warmup.
[Issue #606](https://github.com/kunobi-ninja/kache/issues/606)

Local observations on 2026-07-29:

| Observation                       | Result                                                        | Implication                                                                                                                         |
| --------------------------------- | ------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| Host                              | macOS 26.6, arm64                                             | Kache publishes an Apple Silicon release and its Nix flake supports `aarch64-darwin`.                                               |
| Checkout and `target/` filesystem | `/dev/disk3s5`, APFS, local                                   | The default macOS Kache store under the user's Library cache should be on the same APFS data volume, enabling `clonefile` restores. |
| Current `target/` logical size    | about 15 GiB                                                  | Cross-workspace reuse has meaningful potential, but Kache's separate store also needs a bounded size policy.                        |
| Current environment               | no `RUSTC_WRAPPER`, `CARGO_TARGET_DIR`, or `KACHE_*` override | There is no wrapper conflict or shared-target migration to unwind.                                                                  |
| Jujutsu workspaces                | multiple registered                                           | The use case is already real; the pilot should use two existing disposable workspaces at the same revision where practical.         |

The Rust sources contain compile-time `include_str!` inputs, including files
outside `services/coach-engine`, but no `build.rs`, crate-local `kache.toml`,
`.sqlx`, or migration input was found. Kache says its dep-info pre-pass covers
modules, `include!` targets, and build-script output, so the existing
`include_str!` inputs should be observed. No extra-input declaration is
indicated by this static survey. This must still be validated by changing one
representative included file in a disposable workspace and confirming the
crate misses and rebuilds.
[Kache's source discovery](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/how-it-works/cache-key.mdx#whats-in-the-key),
[ChenChess included skill inputs](../../services/coach-engine/src/local_runtime/skill.rs),
[ChenChess included runtime notice](../../services/coach-engine/src/local_runtime.rs)

Kache complements rather than replaces Turbo's task cache. It only helps when
Turbo actually invokes Cargo; a Turbo task hit bypasses the compiler wrapper
entirely. Conversely, when an otherwise identical Rust task runs in a different
workspace with a cold `target/`, Kache can restore compiler artifacts even if
that workspace has no useful Turbo state.

## Filesystem and OS behavior

| Layout                                                | Restore behavior                                                                                                                 | Suitability                                    |
| ----------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------- |
| macOS APFS, store and target on the same volume       | Reflink through `clonefile`; independent inode, shared CoW blocks                                                                | Preferred for the current host                 |
| Linux btrfs or reflink-enabled XFS, same volume       | Reflink; independent inode, shared CoW blocks                                                                                    | Preferred                                      |
| Linux ext4 without reflink or tmpfs, same filesystem  | Hardlink for immutable `.rlib`, `.rmeta`, and dep-info; copy for mutable/OS-loaded output                                        | Works, with shared-inode caveats               |
| Store and target on different volumes/mounts          | Reflink and hardlink cannot cross the boundary; Kache copies                                                                     | Correct but loses zero-copy/disk-dedup benefit |
| Windows ReFS Dev Drive                                | Block clone                                                                                                                      | Preferred Windows layout                       |
| Windows NTFS                                          | Copy by default; hardlinks require explicit opt-in and can make restored files undeletable or allow mutation to affect the store | Leave the opt-in off                           |
| NFS, SMB/CIFS, 9p, FUSE/virtiofs as `KACHE_CACHE_DIR` | Unsafe for the WAL SQLite local index, especially across machines/OSes                                                           | Do not use                                     |

Kache attempts the reflink first on all supported platforms. On Unix
non-CoW filesystems it hardlinks only immutable artifact classes and copies if
the hardlink fails, including cross-filesystem failure. It copies mutable
outputs rather than letting a later strip, code-sign, or rewrite operation
mutate the cache blob.
[Deduplication behavior](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/deduplication.mdx#zero-copy-restores),
[`hardlink_or_copy`](https://github.com/kunobi-ninja/kache/blob/v0.12.0/src/link.rs#L195-L220)

The local store must remain on fast, single-machine local storage. Kache's
`index.db` uses SQLite WAL and shared memory; upstream warns that a cache
directory mounted into another OS/container or placed on a network filesystem
can corrupt. v0.12.0 detects known non-local filesystems, warns, and can
quarantine and rebuild a corrupt derived index from blob metadata, but recovery
is not permission to use an unsupported layout. If sharing across machines is
needed later, every machine needs its own local `KACHE_CACHE_DIR`; only a
separately configured S3 or filesystem _remote_ belongs on shared storage.
[Container/network guidance](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/getting-started/configuration.mdx#containers-and-cross-compilation),
[filesystem-remote layout](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/remote-cache/filesystem-setup.mdx),
[corruption report #412](https://github.com/kunobi-ninja/kache/issues/412),
[follow-up #415](https://github.com/kunobi-ninja/kache/issues/415),
[`cache_fs.rs`](https://github.com/kunobi-ninja/kache/blob/v0.12.0/src/cache_fs.rs#L1-L15)

On APFS it is normal for Kache's monitor to report no hardlinks. Reflinks have
independent inodes, so hardlink counts stay at one; the cross-platform `Dedup`
metric is the relevant storage indicator.
[Monitor interpretation](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/deduplication.mdx#the-dedup-line-in-the-monitor)

`kache clean` must not be the ChenChess cleanup owner. On macOS, its recursive
scanner deliberately skips TCC-protected locations including `~/Documents`;
this checkout is under `~/Documents`, so the command will not discover this
repository's targets. Continue to use explicitly scoped Cargo/workspace cleanup
when cleanup is actually requested.
[Kache clean's macOS exclusions](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/commands/reference.mdx#kache-clean)

## Concurrency and correctness

### Same-host concurrency

Same-host parallel worktrees are an intended topology. Kache uses a per-key
build lock so one rustc process compiles a missing key while contenders wait
and restore the winner's result. Lock publication is no-clobber and stale-lock
recovery is serialized. The SQLite index runs in WAL mode with a five-second
busy timeout; upstream states it is designed for more than 300 parallel rustc
wrapper processes.
[Architecture](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/how-it-works/architecture.mdx#the-wrapper),
[`PreparedKeyLock`](https://github.com/kunobi-ninja/kache/blob/v0.12.0/src/store.rs#L614-L657),
[`initialize_db`](https://github.com/kunobi-ninja/kache/blob/v0.12.0/src/store.rs#L843-L870)

Blob and metadata publication uses temporary files, file flushes, and atomic
rename. A concurrent writer that wins publication can be treated as a benign
lost race. Store blobs are content-addressed and marked read-only; mutable
restores are independent files, while hardlink-eligible immutable restores
share a read-only inode only on a non-CoW fallback path.
[`atomic.rs`](https://github.com/kunobi-ninja/kache/blob/v0.12.0/src/atomic.rs#L78-L169),
[`store.rs` blob guard](https://github.com/kunobi-ninja/kache/blob/v0.12.0/src/store.rs#L20-L133)

These mechanisms make concurrent workspaces plausible, but the rollout still
needs a deliberate two-process cold-cache race. Product correctness should not
rest only on upstream unit tests or documentation.

### Cache-key boundaries

Kache explicitly frames a false-positive key as build corruption and a
false-negative key as wasted time. Its documented key is broad, but it cannot
observe every possible toolchain or build input.
[Correctness model](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/how-it-works/cache-key.mdx#cache-key)

Known boundaries:

- Proc macros can read process environment variables that rustc and Cargo do
  not expose as stable tracked inputs. Upstream issue #635 demonstrates two
  byte-identical rustc command lines producing semantically different
  artifacts depending on such an environment variable, followed by a stale
  hit. Kache v0.12.0 has no documented full-environment strict mode. A
  crate-source exclusion can mitigate a known offender, and a salt can include
  known semantic environment state, but neither proves coverage of an unknown
  proc macro. This is the reason the pilot must remain opt-in and outside
  release gates.
  [Issue #635](https://github.com/kunobi-ninja/kache/issues/635),
  [source-exclusion configuration](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/getting-started/configuration.mdx#excluding-sources)
- A custom libc/sysroot, same-version distro patch, hidden linker change, or Nix
  store rebuild can change output without changing the version banners Kache
  observes. `KACHE_KEY_SALT` exists to force a cold namespace when that
  surrounding toolchain closure changes.
  [Cache-key salt](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/getting-started/configuration.mdx#cache-key-salt)
- Compile-time inputs that rustc does not report need a crate-local
  `kache.toml` `extra_inputs` declaration. Cargo must also be told
  `cargo:rerun-if-changed`; otherwise rustc is not reinvoked and no wrapper can
  recompute the key.
  [Extra inputs](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/getting-started/configuration.mdx#extra-cache-key-inputs)
- Extra path prefixes and path-only environment variables merge path
  distinctions. Over-broad or inconsistently configured roots can make
  different inputs look identical. Do not set `KACHE_BASE_DIR`,
  `paths.base_dirs`, or `KACHE_PATH_ONLY_ENV_VARS` in the initial ChenChess
  setup; default workspace/target normalization covers the use case.
  [Path-prefix warning](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/getting-started/configuration.mdx#extra-path-prefixes)
- Cross-checkout `.rmeta` bytes can still diverge even after argument/path
  normalization, reducing hits and cascading misses. This is a performance
  failure rather than evidence of a false hit, but it can erase most of the
  expected benefit.
  [Issue #330](https://github.com/kunobi-ninja/kache/issues/330),
  [Substrate results and dep-info refusals in issue #431](https://github.com/kunobi-ninja/kache/issues/431)
- Native `-sys` build outputs can bake checkout paths into static libraries,
  and output-locator environment values such as `DEBUG_OUTPUT_DIR` or
  `OUT_DIR` can remain path-sensitive, causing cross-clone misses and expensive
  downstream `extern` cascades. ChenChess has native dependencies, so this
  should be investigated from its own `why-miss` report rather than assumed
  absent. Another upstream benchmark also recorded dep-info pre-pass refusals,
  which safely pass through but reduce coverage.
  [Upstream SurrealDB measurement](https://github.com/kunobi-ninja/kache/issues/471)

### Integrity checking

By default, a restore checks metadata and blob size, not the content hash.
`KACHE_VERIFY_RESTORES=sampled` rehashes about one in sixteen hits;
`always` rehashes every hit at the cost of another complete read. Use `always`
for the pilot's concurrency and equivalence runs, then `sampled` for routine
development if the pilot succeeds. `kache doctor --verify --checksums` provides
an explicit full-store check, and `--repair` removes corrupt entries.
[Restore verification](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/getting-started/configuration.mdx#restore-verification),
[doctor behavior](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/getting-started/quick-start.mdx#verify-the-setup)

Hash verification catches disk/store corruption. It does not prove that an
under-keyed entry represents every hidden compiler input; the salt,
extra-input, and clean-build controls remain necessary. In particular, it
cannot detect issue #635's environment-sensitive proc-macro stale hit.

## Proposed setup

### Phase 1: repository-scoped local pilot

1. Add Kache v0.12.0 as a pinned flake input and add
   `kache.packages.${system}.default` to a new `devShells.kache`. Leave
   `devShells.default` unchanged. Let `flake.lock` pin the exact upstream
   commit. Upstream's flake exports packages for `aarch64-darwin`,
   `aarch64-linux`, and `x86_64-linux`; it does not publish a Nix binary cache,
   so budget one local source build of Kache and its project-specific Nix
   closure.
   [Upstream Nix installation](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/getting-started/installation.mdx#nix)
   [Upstream binary-cache gap](https://github.com/kunobi-ninja/kache/issues/606)
2. Set the Nix shell's `RUSTC_WRAPPER` to the absolute Nix-store Kache binary.
   This applies only in `nix develop .#kache`. Do not run `kache init`; that
   would mutate global Cargo configuration and install a daemon outside this
   repository's declared development environment.
3. Commit a small `.kache.toml` with:

   ```toml
   [cache]
   local_only = true
   local_max_size = "50GiB"
   cache_executables = false
   ```

   `local_only` makes the initial setup hermetic even if the developer has
   remote/planner settings elsewhere. The size matches Kache's default but
   records the intended cap. `cache_executables = false` makes the macOS
   debug-information decision explicit.
   [Configuration precedence and options](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/getting-started/configuration.mdx#configuration)

4. Set `KACHE_VERIFY_RESTORES=sampled` in the development shell after the pilot.
   During pilot commands, override it to `always`.
5. Derive `KACHE_KEY_SALT` from the toolchain environment, not the workspace
   path. At minimum it should change with the locked nixpkgs revision and Rust
   toolchain version; a dev-shell closure digest is stronger. Every workspace
   must receive the same salt for the same toolchain or hits will be
   partitioned.
6. Leave `KACHE_CACHE_DIR` at its macOS default
   (`~/Library/Caches/kache`) after confirming it is local APFS and on the same
   data volume as the usual workspaces. Do not put it under the repository,
   inside a worktree, on NFS, or in a host/container shared home.
7. Keep each checkout's ordinary `target/`. Do not set a shared
   `CARGO_TARGET_DIR`.
8. Leave `KACHE_RUSTC_PATH_NORMALIZE` enabled and do not add custom base-dir or
   path-only normalization.
9. Do not start the daemon in phase 1. It is unnecessary for the local cache,
   and excluding it makes the evaluation strictly about same-host artifact
   reuse.
10. Keep `release:gate`, `release:proof`, Local Pipeline Runtime builds, and
    release publication in `devShells.default`, where `RUSTC_WRAPPER` remains
    unset. Add a fail-closed guard to the release-process entry point that
    rejects a non-empty `RUSTC_WRAPPER`; this prevents invoking a release
    command from `devShells.kache` from silently caching it.

### Phase 2: promotion only after acceptance

- Document `nix develop .#kache` as an optional acceleration path; keep ordinary
  `nix develop` and release commands uncached while issue #635 lacks a complete
  mitigation.
- If the performance gates pass and issue #635 is either fixed upstream or
  covered by a separately reviewed ChenChess correctness policy, promotion can
  move the wrapper into `devShells.default`. Release gates remain a separate
  policy decision and stay uncached until explicitly approved with clean-build
  evidence.
- Document the LLDB source map:

  ```text
  settings set target.source-map /kache/workspace /absolute/path/to/current/workspace
  ```

- Consider `CC="kache cc"` and `CXX="kache c++"` as a separate experiment for
  Cargo native dependencies. Kache's C/C++ cache is conservative and
  local-only; unsupported flags and compiler shapes pass through. It should not
  be bundled into the Rust pilot because it changes the miss surface and makes
  attribution harder.
  [C/C++ scope](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/getting-started/c-cpp.mdx)
- Consider a daemon or remote backend only if cross-machine sharing becomes a
  separate requirement. Treat all filesystem-remote writers as trusted; Kache
  documents that the shared directory is not a security boundary.
  [Filesystem-remote trust model](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/remote-cache/filesystem-setup.mdx#minimal-configuration)
- Reconsider macOS executable caching only after the `.dSYM`/relocatable-debug
  limitation is resolved and a ChenChess-specific debug/codesign test passes.

## Rollout and verification plan

### 1. Preserve a no-Kache baseline

Measure the same pinned revision and command with distinct empty target
directories:

- vanilla Cargo with `RUSTC_WRAPPER` genuinely unset;
- Kache with an empty task-local cache;
- Kache in a second workspace with another empty target and the populated
  cache.

Do not use `KACHE_DISABLED=1` as the vanilla performance baseline. Even when
disabled, the wrapper strips Cargo's incremental flag, so that measures Kache
passthrough rather than normal Cargo.
[Disabled-wrapper behavior](https://github.com/kunobi-ninja/kache/blob/v0.12.0/docs/getting-started/quick-start.mdx#disabling-kache-temporarily)

Use task-local `CARGO_TARGET_DIR` values instead of `cargo clean` on either
developer workspace. Start with:

```sh
cargo test --workspace --no-run
```

Then repeat the repository's curated Rust test command from
[`docs/local-ci.md`](../local-ci.md#rust-release-regression-and-focused-certification).
Record wall time, user/system CPU, Kache hit/dup/miss/passthrough counts,
reflink/hardlink/copy bytes, and logical/physical cache size.

### 2. Prove cross-workspace key reuse

Use two disposable Git worktrees or Jujutsu workspaces at the exact same
revision. Give each its own empty target directory and both the same empty,
task-local Kache cache.

1. Build workspace A to populate the store.
2. Build workspace B with `KACHE_PROGRESS=verbose`.
3. Run `kache report` and `kache stats`.
4. Use `kache why-miss <crate>` for the most expensive unexpected misses.
5. Confirm the report attributes restores to reflinks on this APFS host. A
   hardlink count of zero is expected; copy-restored library bytes are not.
6. Run `kache doctor --verify --checksums`.

Upstream's own benchmark deliberately uses a cold build in one checkout and a
warm build in another absolute path, so this mirrors Kache's intended
worktree/agent-clone scenario while keeping the verdict ChenChess-specific.
[Benchmark shape](https://github.com/kunobi-ninja/kache/blob/v0.12.0/README.md#benchmarks)

### 3. Race two cold workspaces

With another empty task-local cache and distinct empty targets, start the same
build concurrently in A and B. Both must finish successfully. The report should
show that contenders either restored a per-key winner or safely compiled their
own misses; it must show no SQLite busy/locked/corrupt error. Finish with full
checksum verification.

Repeat once while interrupting one workspace's build. The surviving build must
complete, and a later build must recover any stale per-key marker. This tests
the exact multi-agent failure mode rather than only clean parallel completion.

### 4. Prove invalidation

In a disposable workspace, make one change in each category and confirm the
owning crate misses while unrelated stable dependencies hit:

- a Rust source change;
- one `include_str!` file under `skills/chenchess-coach`;
- a feature or `RUSTFLAGS` change;
- the proposed toolchain salt;
- a dependency-lock change, if it can be created without broad unrelated
  updates.

Revert each disposable change before the next case. Run the relevant tests and
compare the generated/output behavior, not only the cache event label.

### 5. Measure the tight edit loop

In one disposable workspace, compare five representative edit/build cycles
with vanilla Cargo incremental compilation and with Kache. Include a small leaf
edit and a central library edit. The Kache rollout fails if cross-workspace
benefit is bought with an unacceptable routine edit-loop regression.

### 6. Run repository validation

After the configuration change itself:

1. run `kache doctor`;
2. run the curated Rust suite;
3. run the relevant Turbo Rust tasks from a cold target and again from a second
   workspace inside the opt-in shell;
4. run a Nix flake evaluation/check sufficient to prove that both
   `devShells.default` and `devShells.kache` instantiate on the supported host;
5. leave the opt-in shell and run the same focused Rust validation from
   `devShells.default`, asserting that `RUSTC_WRAPPER` is unset.

The release scripts currently clone `process.env` and remove provider secrets
and overrides, but do not remove `RUSTC_WRAPPER`; the release CLI build uses
that cleaned environment. Enabling the wrapper in `nix develop` would therefore
also enable it in the release proof unless the scripts make an explicit
policy choice.
[`cleanEnvironment`](../../tooling/scripts/release-process.ts),
[`release-cli-build`](../../tooling/scripts/release-proof.ts)

Do not run `release:proof` merely for this pilot; repository policy reserves it
for an explicit whole-repository audit, Local Pipeline Runtime
publication/certification, or another named release procedure. During and after
the pilot, the fail-closed release guard must reject a non-empty
`RUSTC_WRAPPER`, and those procedures must run from `devShells.default`. The
optional development shell can evaluate Kache without making release
correctness depend on a cache with a demonstrated semantic stale-hit class.
[ChenChess release-proof policy](../local-ci.md#validate-implementation-work)

### 7. Acceptance criteria

Accept continued opt-in local use only if all are true:

- both same-revision workspaces pass the curated Rust suite with checksum
  verification;
- the concurrent cold-cache and interrupted-builder cases produce no corrupt
  entry, stuck lock, or database error;
- source, included-file, flag, salt, and dependency invalidation behave as
  expected;
- the second-workspace run has a material wall-time improvement, not just a
  high count of cheap hits; use compile-time-weighted misses when deciding;
- APFS restores are reflinks and avoid material copy-restored library bytes;
- the ordinary edit loop is acceptable against vanilla Cargo incremental
  compilation;
- a clean build with the wrapper unset still passes, proving the cache is an
  optimization rather than a build prerequisite;
- release gates refuse or remove `RUSTC_WRAPPER`;
- the default-off macOS executable policy remains in force.

Do not predeclare a percentage threshold before the baseline exists. Report
absolute wall time, weighted compile time saved, and storage cost, then choose a
threshold from the observed ChenChess distribution. Promotion into
`devShells.default` additionally requires the issue #635 correctness condition
in phase 2; passing the performance criteria alone is insufficient.

### 8. Rollback

Rollback must remove or unset `RUSTC_WRAPPER`, not merely set
`KACHE_DISABLED=1`, because disabled Kache still removes Cargo's incremental
flag. Removing the wrapper immediately restores ordinary per-workspace Cargo
behavior; existing `target/` directories remain usable. The local store is
derived state and can be garbage-collected later, but it does not need to be
deleted to disable the integration.

## Risks and mitigations

| Risk                                                                                    | Consequence                                                   | Mitigation                                                                                              |
| --------------------------------------------------------------------------------------- | ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| Kache is a fast-moving 0.x project; v0.12.0 was released one day before this assessment | Behavior and config can change quickly                        | Pin the flake revision; upgrade deliberately with the same benchmark                                    |
| An environment-sensitive proc macro is absent from the key (#635)                       | Semantic stale hit; potentially a silently wrong binary       | Keep Kache opt-in and outside release gates; exclude a known offender; preserve uncached validation     |
| `.rmeta` or native static libraries differ across checkout paths                        | Low hit rate and little speedup                               | Gate on two-workspace weighted results; use `why-miss`; reject if benefit is immaterial                 |
| Incremental compilation is stripped                                                     | Slower same-workspace edit loop                               | Benchmark representative edits; rollback by unsetting the wrapper                                       |
| macOS executables are not cached by default                                             | Final link/test harnesses remain expensive                    | Accept as initial safety boundary; do not opt in merely to improve a headline metric                    |
| Upstream Nix has no binary cache (#606)                                                 | First opt-in shell builds Kache and its Rust closure locally  | Treat as measured one-time host setup cost; rely on the shared local Nix store afterward                |
| Hidden compile-time inputs or invisible toolchain changes                               | Stale/wrong restored artifact                                 | Audit extra inputs, add `rerun-if-changed`, derive a toolchain salt, preserve clean uncached validation |
| Store on network/virtual storage                                                        | WAL corruption                                                | Keep one local store per machine; remote backend only for cross-machine sharing                         |
| Store and target on different volumes                                                   | Correct but copy-heavy restores and extra disk                | Co-locate on APFS; detect copy bytes during verification                                                |
| Shared-inode fallback on non-CoW Unix filesystems                                       | Mutation can affect a stored immutable blob                   | Rely on Kache's read-only guard, use restore verification, keep mutable outputs on copy strategy        |
| Local cache contains compiled bytes that may embed compile-time data                    | Data persists outside a workspace and could be uploaded later | Start with `local_only = true`; do not compile secrets through `env!`; review before adding a remote    |
| Release proof silently inherits the wrapper                                             | Certified binary may come from the pilot cache                | Explicitly choose release-cache policy; retain an uncached proof/build during rollout                   |
| `kache clean` skips this `~/Documents` checkout                                         | Targets grow despite believing cleanup covered them           | Keep cleanup explicitly workspace-scoped and documented                                                 |

## Decision boundary

Kache is technically aligned with separate Git/Jujutsu workspaces and the
current APFS host, and it is safer than sharing one Cargo target directory. It
is not justified as an unconditional repository default or release-gate
wrapper. The environment-sensitive proc-macro stale hit, Rust cross-clone miss
evidence, macOS executable limitation, disabled incremental compilation,
missing upstream Nix binary cache, and v0.12.0's recency make a pinned,
no-daemon, named opt-in local shell the correct next action.
