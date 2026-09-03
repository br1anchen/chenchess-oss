# Local Coach Skill

The Local Coach Skill lets Codex or Claude Code request deterministic chess facts from a checkout-independent local CLI. Docker is the only host runtime prerequisite on the certified target: Apple Silicon macOS. Stockfish, Maia-2, PyTorch, model files, generated wire definitions, and the shared skill are installed as one pinned user-level unit.

## Installation size and network work

Installation copies the `chenchess` CLI and skill, downloads the checksum-pinned Stockfish archive, pulls the digest-pinned multi-platform Maia image, and provisions the checksum-pinned Maia `rapid` model. These steps require network access and several gigabytes of free space for the image, model, Docker layers, and one retained rollback unit. Exact byte counts vary with Docker's existing layer cache; inspect them with `docker system df`, `docker image inspect`, and `chenchess runtime maia-status` before and after installation.

Normal reviews do no network provisioning. They reuse the installed Stockfish binary, model volume, and Maia container. The Maia service remains running for warm reviews until explicitly stopped or uninstalled.

## Install and operate

```sh
chenchess runtime install --manifest /path/to/runtime-manifest.json
chenchess runtime doctor
chenchess runtime maia-status
chenchess runtime maia-stop
chenchess runtime maia-start
chenchess runtime uninstall
```

Installation writes only per-user paths: `~/.local/bin`, `~/.config/chenchess`, `~/.local/share/chenchess`, `~/.local/state/chenchess`, `~/.agents/skills`, and `~/.claude/skills`. It requires no administrator privileges. Updates stage and verify a complete unit, run a live smoke review, switch atomically, and retain the prior unit until activation succeeds.

## Local computation and hosted agent data

PGN parsing, Stockfish analysis, Maia inference, Critical Moment selection, Rule Extraction, and Draft Game Review validation run locally. A local PGN file path is passed directly to the CLI; the skill tells the host agent not to read the raw file into its context.

The terminal `gameImported` Review Session event returns the grounded Game Review to the configured coding agent so it can write prose. If Codex or Claude Code uses a hosted model, those structured review facts—and pasted PGN already supplied in the conversation—may leave the machine under that provider's data policy. The skill does not claim fully offline agent prose generation, does not call a separate language provider, and does not require an additional credential.

## Supported limits and resources

The maximum is 400 plies. Provider work is limited to 30 seconds per position, runtime startup to 600 seconds, a live command to four hours, and cancellation cleanup to five seconds. Live evaluation allows 15 centipawns and 0.02 probability of numeric drift while requiring exact moves, ranks, categories, selected plies, and provenance. Certification records cold and warm time plus Maia CPU and memory output; `chenchess runtime maia-status` shows current service state and resource use.

Only one live review, live evaluation, or certification owns the runtime at a time. A second caller receives `runtime busy`. Deterministic recorded-evidence evaluation remains available without Docker or the live lock.

## Recovery

Run `chenchess runtime doctor` after an interrupted install, failed update, Docker restart, provider error, or incompatible runtime manifest. Pending installation recovery converges on the last healthy active unit. A failed update leaves that unit active. For checksum damage or incompatible runtime metadata, use the same atomic install command with a trusted manifest; do not edit runtime files by hand.

Use `chenchess runtime maia-stop` to release the Maia container's active resources. Use `chenchess runtime uninstall` to remove chenchess-owned links, units, containers, and volumes without deleting unrelated Docker data. If Docker itself is unavailable, restore Docker first and rerun `doctor` or `uninstall`.

## Verification

`./tooling/nix-develop --command bun run test` runs the repository's tests: Rust
formatting, lint, tests and build through Turborepo, the Maia service tests, and
the frontend lint, typecheck, tests and build. The whole-repository release
proof that additionally exercised a published runtime — fresh installation, warm
reuse, failed-update rollback, measured certification, clean uninstall — is
release machinery this snapshot does not carry.

Manual checks use the fixed skill contract in `skills/chenchess-coach/SKILL.md`: pasted PGN, a local PGN path, White/Black/both Review Sides, simple/standard/advanced Explanation Styles, one validation repair, a selected-moment follow-up, a stopped or damaged runtime, and a malformed Review Session command. Codex results are recorded manually because the local proof intentionally has no hosted-model credentials. Claude Code remains a supported discovery target, but its manual host check is deferred until a first-party account is available and must not be represented as completed evidence before then.
