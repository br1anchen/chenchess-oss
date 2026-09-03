# System architecture

Living diagrams of the ChenChess system. Update the module doc when its
structure changes; vocabulary follows `CONTEXT.md` and decisions live in
`docs/adr/`.

- [coach-engine.md](coach-engine.md) — the Rust application service
- [central-host.md](central-host.md) — the public Node origin and MCP layer
- [maia.md](maia.md) — Maia-2 inference service
- [packages.md](packages.md) — shared packages and the generated contract

## One-screen overview

```mermaid
flowchart LR
    subgraph Surfaces
        B[Player browser]
        H["ChatGPT / Claude<br/>(MCP host)"]
        S["Coach Skill<br/>(local chenchess CLI)"]
    end

    subgraph central-host["apps/central-host (public Node)"]
        ST[Static Vite surfaces]
        RY["/api byte relay"]
        MCP["MCP endpoint + Coach OAuth"]
        ART["Coach App ui:// artifacts"]
    end

    subgraph coach-engine["services/coach-engine (private Rust/Axum)"]
        RS[Review Session Processor]
        SF[Stockfish UCI child]
        LL[Hosted Language Layer]
        FS[(Firestore)]
    end

    M["services/maia<br/>(private Python HTTP)"]

    B -->|HTTPS + Firebase ID token| RY
    B --> ST
    H -->|OAuth + PKCE, MCP tools| MCP
    MCP --> ART
    RY -->|HTTP| RS
    MCP -->|command union over HTTP| RS
    S -->|JSONL over stdio, local runtime| RS
    RS --> SF
    RS -->|HTTP| M
    RS --> LL
    RS --> FS
```

Three Player surfaces converge on one Rust service through one generated
command/event contract (`@chenchess/coach-engine-sdk`). Coach Engine is the
only writer of application Firestore data and the only place chess claims are
produced; every surface is a projection of its events.

## The grounded review pipeline

```mermaid
sequenceDiagram
    participant P as Player surface
    participant CH as central-host
    participant CE as coach-engine
    participant SF as Stockfish
    participant MA as Maia
    participant LL as Language Layer

    P->>CH: ReviewSessionCommand (import Game)
    CH->>CE: relay / project to command
    CE->>CE: parse PGN, persist durable Game Import
    loop every position
        CE->>SF: UCI evaluate
        CE->>MA: human-move prediction
    end
    CE->>CE: Rule Extractor selects Critical Moments (deterministic)
    CE->>LL: author comments from typed causal facts
    CE->>CE: validation boundary (Grounding Gate)
    CE-->>CH: ReviewSessionEventEnvelope stream (accepted → progress → terminal)
    CH-->>P: projected presentation
```

Chess truth comes only from engines and deterministic rules; the Language
Layer phrases prose over typed facts and its output is validated before
publication (ADR 0009, 0014, 0016).

## Repository layout

```text
/
├── apps/
│   ├── central-host/   # public Node origin: web app, MCP, OAuth, relay
├── services/
│   ├── coach-engine/   # Rust workspace: app crate + crates/{contract,pipeline}
│   └── maia/           # Python: Maia-2 position inference
├── packages/
│   ├── coach-engine-sdk/   # GENERATED contract (types, decoders, schema)
│   ├── review-projection/  # pure contract → presentation projection
│   ├── shared-assets/      # Canonical Game, grounding sentences, limits
│   └── ui/                 # host-neutral shared presentation layer
├── skills/chenchess-coach/ # Player-facing local Coach Skill
└── tooling/                # release gates, certification, repo checks
```

Boundary rule (`CODING_STANDARDS.md`): `apps/` and `services/`
never read each other; anything shared lives in `packages/` and is imported
from both sides.
