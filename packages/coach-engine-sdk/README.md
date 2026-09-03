# `@chenchess/coach-engine-sdk`

This package is generated from `services/coach-engine/src/review_session_contract/`. It publishes the authoritative TypeScript types, decoders, JSON Schema, and positive fixtures derived from the canonical Game captured under `packages/shared-assets/fixtures/Synthet1`.

Regenerate with:

```sh
cargo run -p chen-chess-coach-engine --bin generate_review_session_contract
```

Verify checked-in artifacts without changing them with:

```sh
cargo run -p chen-chess-coach-engine --bin generate_review_session_contract -- --check
```

Do not edit generated files directly. `commands.json` proves that web, Coach Skill, and Coach App inputs share one command union while keeping surface ownership explicit. `events.json` covers accepted, progress, and every terminal outcome. Evaluation recordings are intentionally kept outside this product contract.

The schema defines the shared operation vocabulary used by the Review Engine, web, and Coach surfaces. Handler rollout may add behavior behind these shapes, but it must not create parallel or provisional wire commands.

`CoachEngineClient` accepts an auth-neutral async credential provider and calls it
for every command. A web adapter can supply a current Firebase ID token while a
protocol adapter can supply a Coach access token; the SDK implements neither
authentication system and stores neither credential.

Lichess import accepts only `https://lichess.org/<8-or-12-character-game-id>` with an optional `/white` or `/black` suffix. A suffix preselects the Review Side without identifying a Player; a bare URL requires an explicit White or Black selection. The importer makes one anonymous request to the fixed single-Game PGN export endpoint with clocks, evaluations, accuracy, phase division, and generated prose excluded, then builds the same immutable `ImportedGame` used by pasted and local PGN inputs.

Chess.com import accepts only shared Game URLs in the forms `https://www.chess.com/game/computer/<numeric-id>`, `https://www.chess.com/game/daily/<numeric-id>`, and `https://www.chess.com/game/live/<numeric-id>`, published as the `CHESS_COM_GAME_URL_PATTERN` this package exports so a surface admits what the Engine imports. It requires an explicit White or Black Review Side, reads the completed standard Game from the URL-kind-specific fixed-origin callback, verifies that computer Games contain exactly one computer Player while live and daily PvP Games contain none and that a daily Game carries a days-per-turn clock, legally decodes the compact move list, and records versioned Chess.com response and PGN digests in the immutable `ImportedGame`.

Provider refresh is deliberately separate from ordinary generation:

```sh
cargo run -p chen-chess-coach-engine --bin capture_review_session_recording -- --capture
cargo run -p chen-chess-coach-engine --bin capture_review_session_recording -- --capture --accept
```

The first command shows the recording diff without changing the accepted fixture. The second is the explicit acceptance operation.

Content digests use RFC 8785 JSON canonicalization. The generated TypeScript decoders are asynchronous because they verify SHA-256 identities with Web Crypto before returning branded contract values.
