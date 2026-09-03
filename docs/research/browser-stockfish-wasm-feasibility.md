# Browser Stockfish (WebAssembly) feasibility

Research date: 2026-08-30. Gathers the external facts
[Plan 006](../plans/006-speed-up-move-exploration-and-review-loading.md) Phase 5
depends on. It establishes what is buildable and at what cost; it does **not**
decide whether ChenChess should do it. That decision is gated on the root
`LICENSE` choice in
#521.

## Evidence labels

- **[M]** — measured during this spike on an Apple M1 Max under Node 23.11 (V8),
  2026-08-30. These are first-party measurements on one machine, not published
  benchmarks, and V8 approximates Chrome rather than Safari's JSC.
- **[S]** — read directly from package source, `registry.npmjs.org`, or a
  vendor's shipped files.
- **UNVERIFIED** — stated as unknown rather than estimated.

Published time-to-depth benchmarks for WebAssembly Stockfish **do not appear to
exist**. That is a negative search result across the nmrugg and lichess-org
repositories and general search, not an omission. Every timing below is [M].

## Finding

A **single-threaded** Stockfish 18 build with a small embedded network is fast
enough for provisional evaluation, needs **no cross-origin isolation**, and
ships as one ~7.3 MB file. The performance folklore about WebAssembly Stockfish
is wrong in our favour; the licensing position is the real constraint.

## Which packages are alive

| npm package | Version | Wraps | Declared licence | Status |
| --- | --- | --- | --- | --- |
| `stockfish` (nmrugg) | 18.0.8, 2026-06-15 | Stockfish 18 | `GPL-3.0` | Maintained; funded by Chess.com |
| `@lichess-org/stockfish-web` | 0.4.2, 2026-07-13 | SF 18, SF 18 threat-small, Fairy-SF 14 | `AGPL-3.0-or-later` (see below) | Maintained; powers lichess.org |
| `stockfish.js` (niklasf) | 10.0.2, **2019** | SF 10 | GPL-3.0 | Dead; lichess last-resort fallback only |
| `stockfish.wasm` (niklasf) | 0.10.0, **2021** | SF 11 HCE | GPL-3.0 | Dead |

[S] via `registry.npmjs.org`. Note the naming trap: the **npm package named
`stockfish.js` is not nmrugg's and is seven years stale**; nmrugg's package is
named `stockfish`. What older notes call `lila-stockfish-web` is now
[lichess-org/stockfish-web](https://github.com/lichess-org/stockfish-web).

## Licensing

**Stockfish is GPL-3.0, and both live wrappers carry copyleft.** ChenChess today
redistributes none of it: the engine is a separately downloaded native binary
invoked over UCI (`runtime/THIRD_PARTY_NOTICES.md:5-7`). Shipping a WASM build
means redistributing GPL-3.0 code inside the client artifact.

- nmrugg `stockfish` 18.0.8: `Copying.txt` is GPLv3; README states
  "Stockfish.js (c) 2026, Chess.com, LLC / GPLv3". [S]
- `@lichess-org/stockfish-web` **ships contradictory licence metadata**:
  `package.json` declares `AGPL-3.0-or-later`, while the `LICENSE` file in both
  the published tarball and the repository is plain **GPL-3.0**, and GitHub's
  licence detection reports `GPL-3.0`. [M, by diffing the shipped file] AGPL
  would additionally reach network-served use. **Do not adopt this package
  without resolving the contradiction with its maintainers.**

Both lichess and chess.com ship the engine as a separate unmodified artifact
driven over the arms-length UCI text protocol in a Worker, rather than linking
it into an application chunk. Whether that boundary is sufficient is a legal
question this note does not answer. The practical consequence either way: **do
not let a bundler inline the engine into an app chunk.**

## Threading and cross-origin isolation

| Build | Needs `SharedArrayBuffer` | Needs COOP/COEP |
| --- | --- | --- |
| `stockfish-18.js`, `-lite.js` (nmrugg, multi-threaded) | Yes | Yes |
| **`stockfish-18-single.js`, `-lite-single.js`** | **No** | **No** |
| `stockfish-18-asm.js` | No | No |
| Every `@lichess-org/stockfish-web` NNUE build | Yes | Yes |

Lichess's engine table declares `requires: ['sharedMem', 'simd',
'dynamicImportFromWorker']` for all four of its modern builds; only its
Stockfish 10/11 hand-crafted-evaluation fallbacks require neither. [S] So on
lichess's ladder, losing isolation drops you from SF 18 NNUE to SF 11 HCE.
**nmrugg's single-threaded builds are the escape from that trade** — same
Stockfish 18, same embedded net, one thread, no headers.

### Why isolation is a trap for an authenticated surface

`COOP: same-origin` **breaks Firebase `signInWithPopup`**, structurally: the SDK
loads a cross-origin iframe from `<project>.firebaseapp.com`, and Google serves
no COEP/CORP on it.
[firebase-js-sdk#6467](https://github.com/firebase/firebase-js-sdk/issues/6467)
has been open since 2022-07-22 with a maintainer reply that this is expected
behaviour and not on their timeline. `__/auth/handler` is not open source.

- `same-origin-allow-popups` does **not** grant isolation. The combination that
  would is a separate Chrome proposal,
  [5731309970259968](https://chromestatus.com/feature/5731309970259968), status
  "No active development". `restrict-properties` is "On hold".
- `COEP: credentialless` is **unsupported in Safari, all versions through 27**
  ([WebKit bug 230550](https://bugs.webkit.org/show_bug.cgi?id=230550), status
  `NEW`), and does not fix iframes anyway.
- `Document-Isolation-Policy` (Chrome 137) grants per-frame isolation without
  COOP/COEP, but is **Chrome desktop only** with a negative WebKit standards
  position.
- Under `require-corp`, cross-origin no-CORS subresources are blocked outright
  and `document.domain` is disabled.
- `coi-serviceworker` last shipped in **2023-12** and is unmaintained; its
  `credentialless` path is a no-op on Safari.

Live headers, measured 2026-08-30 [M]: lichess.org uses
`COOP: same-origin` + `COEP: credentialless` site-wide. chess.com uses
`COOP: same-origin` + `COEP: require-corp` **only on `/analysis`**, while its
home page uses `same-origin-allow-popups` with no COEP. That route split — never
isolating the authenticated shell — is the pattern to copy if threading is ever
wanted.

## Asset size, measured [M]

The two projects package oppositely: **nmrugg embeds the network in the `.wasm`;
lichess fetches networks separately.** Verified by byte-searching the binaries.

nmrugg `stockfish` 18.0.8, single file, no separate net download:

| File | Raw | gzip -9 | Embedded net |
| --- | ---: | ---: | --- |
| `stockfish-18.wasm` (multi-threaded) | 113,007,340 | 75,817,932 | big + small |
| `stockfish-18-single.wasm` | 112,992,459 | 76,524,442 | big + small |
| `stockfish-18-lite.wasm` (multi-threaded) | 7,093,151 | 5,585,676 | `nn-9067e33176e8` |
| **`stockfish-18-lite-single.wasm`** | **7,295,411** | **5,639,195** | `nn-9067e33176e8` |
| `stockfish-18-asm.js` | 10,508,771 | 6,588,431 | — |

An undocumented `--ultra-lite` target using the 3.5 MB `nn-37f18f62d772` exists
in source but is not published to npm. [S]

`@lichess-org/stockfish-web` 0.4.2 — small wasm, large separate network:
`sf_18.wasm` is 601,760 raw / 160,880 brotli, but its `nn-c288c895ea92` big net
is **108,919,594 bytes** (72,754,437 gzip). Lichess caches that net in
OPFS/IndexedDB, which is the only reason a 108 MB download is tolerable — a
one-time cost, not per-session. [S/M]

## Speed, measured [M]

Ruy Lopez Closed middlegame, 8 s fixed, `Hash 16`, one thread, three runs.
Identical `score cp` and `seldepth` across engines confirm the same search.

| Engine | nps | Depth in 8 s |
| --- | ---: | ---: |
| Native SF 18 (arm64 `NEON_DOTPROD`, Homebrew) | 274k–295k | 23 |
| WASM SF 18 big net, single-threaded | 325k–340k | 23 |
| **WASM SF 18 lite, single-threaded** | **789k–1,018k** | **27–29** |

Time to depth, engine-reported, single-threaded, `Hash 256`:

| Build | d10 | d12 | d15 | d18 | d20 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Native SF 18 (non-PGO) | 14 ms | 33 ms | 282 ms | 526 ms | 1,087 ms |
| WASM SF 18 big, 1 thread | 20 ms | 35 ms | 218 ms | 427 ms | 833 ms |
| WASM SF 18 lite, 1 thread | 19 ms | 72 ms | 161 ms | 488 ms | 1,241 ms |

Two caveats that must travel with these numbers. The native baseline is
**handicapped**: Homebrew's formula runs `make build`, not `make profile-build`,
so it has no PGO while the WASM builds do — a properly built native binary would
be faster. And V8 is not JSC.

Even so, the widely repeated claim that WebAssembly Stockfish is "2–3× slower"
than native **did not reproduce**. The frequently cited 1.45–1.55× figure is
from [Jangda et al., 2019](https://arxiv.org/pdf/1901.09056), a general SPEC CPU
result — not Stockfish, and seven years stale. Treat any "WASM Stockfish is N×
slower" claim as unsourced.

**UNVERIFIED — mid-range mobile.** No device data was gathered and no published
figures were found. Do not scale the M1 Max numbers by a guess. A mid-range
Android being 3–10× slower is plausible inference, not measurement, and the
device that most needs a local engine is the one with no evidence behind it.

## Prior art for provisional-local, authoritative-server evaluation

Narrower than the rest of this note. A subagent assigned this question fabricated
citations and its output was discarded entirely; what remains was re-verified by
hand and **this section is not exhaustively searched**.

**Chess.com is the closest verified match**, and it splits exactly on the
asset-size boundary measured above. Its
[support article 9462780](https://support.chess.com/en/articles/9462780-how-do-the-chess-engines-on-chess-com-work)
states that server-side engines are "always modern engines (at the moment
Stockfish 18) with the full NNUE", including Game Review; that "the full NNUE is
108 MB, which we cannot force-download onto every user's machine for performance
reasons"; and that the browser default is "a 'Lite' version". (The article says
lite is hand-crafted-evaluation only, but the shipped lite build embeds
`nn-9067e33176e8` — the documentation appears stale.)

**Lichess does something different, and in the opposite direction.** Client and
server evaluations are separate fields on a node, and arbitration is by node
count, not by server authority — `useFirstEval` returns
`a.nodes >= b.nodes`. [S] More importantly,
[database.lichess.org/#evals](https://database.lichess.org/#evals) describes its
eval cache as "produced by, and for, the Lichess analysis board, running various
flavours of Stockfish within user browsers": **browsers populate the server
cache**, inverting the direction ChenChess is considering.

**Not established:** a documented engineering write-up of a provisional local
evaluation later replaced by an authoritative server evaluation. ChessBase
behaviour was not researched.

## What this means for ChenChess

1. Take `stockfish-18-lite-single` from npm `stockfish` if this ships at all:
   7.3 MB raw / 5.6 MB gzip, one file, net embedded, **no COOP/COEP**, depth 12
   in ~72 ms and depth 15 in ~161 ms on a laptop.
2. Never isolate the authenticated `/app/` shell. Firebase popup sign-in has no
   vendor fix, `credentialless` does not exist in Safari, and
   `Document-Isolation-Policy` is Chrome-desktop-only. If threading is ever
   wanted, isolate a single analysis route as chess.com does.
3. The dominant client-side latency is asset download and instantiation, not
   search. Budget for the 5.6 MB transfer and a warm-up, not for the search.
4. Resolve GPL-3.0 first. It is the only gate here that code cannot route
   around.
