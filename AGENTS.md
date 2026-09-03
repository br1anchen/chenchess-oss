# Working in this repository

This is a published snapshot. It has no history and no upstream to merge into;
read [`CONTRIBUTING.md`](CONTRIBUTING.md) before opening a pull request.

## Run it before you change it

`bun run local:up` starts the whole product on this machine — a Firebase
Authentication clone, a Firestore clone, the Human Move Model, Coach Engine and
Central Host — and `bun run local:seed` gives you a Player who can sign in.
Nothing is deployed and no Google credential is read. [`README.md`](README.md)
has the prerequisites.

## Coding standards

Tests, types, package boundaries, styling, and the rules on defensive code all
live in [`CODING_STANDARDS.md`](CODING_STANDARDS.md). The domain vocabulary is
in [`CONTEXT.md`](CONTEXT.md) and the decisions behind it in
[`docs/adr/`](docs/adr/).

## Two things that will cost you an afternoon otherwise

- **Build Rust through `./tooling/cargo-cached`**, never a bare `cargo`. The
  build cache only sees prefixed invocations, so a bare `cargo` silently builds
  uncached. See [`docs/rust-build-cache.md`](docs/rust-build-cache.md).
- **Enter the development shell through `./tooling/nix-develop`.** It hands Nix
  an ignore-aware source instead of copying `target/` and `node_modules/` into
  the store.

## Documentation placement

Every durable document lives under `docs/`, except `AGENTS.md`, `CONTEXT.md`
and `CODING_STANDARDS.md`.

| Kind                                                                            | Home                                                       |
| ------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| Coding standards — tests, types, package boundaries, and styling                | `CODING_STANDARDS.md`                                      |
| Architecture decision                                                           | `docs/adr/`                                                |
| Specification — decision-complete implementation contract                       | `docs/spec/`                                               |
| Implementation plan handed to an executor                                       | `docs/plans/`                                              |
| Product requirements                                                            | `docs/prd/`                                                |
| Acceptance and certification procedure                                          | `docs/acceptance/`                                         |
| Research note, dated and evidence-labelled                                      | `docs/research/`                                           |
| Prototype write-up                                                              | `docs/prototypes/`                                         |
| Operator runbook or topology guide                                              | `docs/` root                                               |
| UI/UX and brand design                                                          | `docs/design/` — visual design only, never technical specs |
| Working agreements                                                              | `docs/agents/`                                             |

`docs/adr/`, `docs/plans/`, `docs/research/`, `docs/acceptance/` and
`docs/prototypes/` are historical. They are kept as written, so some of them
cite deployment files this snapshot does not carry; that is a record of what was
decided, not an instruction to look for a missing file.
