# Full Pipeline Game Review

## Problem Statement

Amateur Players, especially children, can import games into existing chess tools but often receive engine-centric feedback that is hard to understand, discouraging, or disconnected from the kinds of mistakes players at their Elo Profile are actually likely to make. ChenChess needs to produce Game Reviews that explain not only what the objectively strong move was, but why a human at the Player's Elo Profile might choose a different move and what pattern the Player can learn from that moment.

## Solution

ChenChess will generate Critical Moment Game Reviews from imported PGN Games using the full pipeline: Engine Analysis, Human Move Model output, Rule Extractor facts, and LLM Explainer prose. The MVP will use Stockfish as the objective Engine Analysis provider and Maia as the Human Move Model, both behind Model Adapters. The Player authenticates through Better Auth SSO, sends an Auth Token to the Review Engine as bearer authentication, imports a Game, enters a per-review Elo Profile, selects an Explanation Style, and receives a Game Review tailored to their chess level and wording preference. The Player can also nominate a Player-Selected Moment for on-demand review when their intuition identifies a moment the pipeline did not automatically select.

The system must support local self-hosting and a Central Host deployment. The frontend remains React + Vite, not Next.js. The Review Engine is implemented by the Rust backend; it owns protected application APIs, validates Auth Tokens, orchestrates the chess pipeline, and talks to configured providers and model services.

## User Stories

1. As a Player, I want to sign in with SSO, so that my Game Review requests are protected.
2. As a Player, I want the app to reject unauthenticated Game Review requests, so that only authenticated Players can use protected review endpoints.
3. As a Player, I want to paste PGN from Chess.com, so that I can review games I played there.
4. As a Player, I want to paste PGN from Lichess, so that I can review games I played there.
5. As a Player, I want to import PGN from another platform, so that I am not locked to one chess site.
6. As a Player, I want invalid PGN to produce a clear error, so that I can fix the import.
7. As a Player, I want to enter my Elo Profile per Game Review, so that the review matches my current playing strength.
8. As a Player, I want the pipeline to use my Elo Profile for chess decisions, so that the selected moments reflect what players like me are likely to see or miss.
9. As a Player, I want to choose an Explanation Style, so that I can get simple, standard, or advanced wording without changing the chess analysis.
10. As a child Player, I want simple explanations, so that I can understand the lesson without expert notation.
11. As an adult amateur Player, I want simple explanations, so that the review stays practical and not intimidating.
12. As an advanced Player, I want advanced explanations, so that the review can use more concise chess language.
13. As a Player, I want Stockfish-based Engine Analysis, so that the Game Review has objective chess truth.
14. As a Player, I want Maia-based Human Move Model analysis, so that the review understands what a player at my Elo Profile might plausibly choose.
15. As a Player, I want the review to compare my move, Stockfish's preferred idea, and Maia's human-likely move, so that I can understand the difference between best play and human tendencies.
16. As a Player, I want the Game Review to focus on Critical Moments, so that I am not overwhelmed by every move in the Game.
17. As a Player, I want each Critical Moment to explain the teachable concept, so that I know what pattern to practice.
18. As a Player, I want the review to avoid inventing engine claims, so that I can trust the chess feedback.
19. As a Player, I want the LLM Explainer to use only extracted facts, so that prose does not replace chess analysis.
20. As a Player, I want a short training plan, so that I know what to do after reading the Game Review.
21. As a Player, I want to select a move I thought was critical, so that the pipeline can review my intuition even if it did not choose that move.
22. As a Player, I want Player-Selected Moment review to be on-demand, so that I can explore one moment without rerunning the whole Game Review.
23. As a Player, I want Player-Selected Moments to stay in-session for MVP, so that I can use the feature without creating persisted feedback records.
24. As a self-hoster, I want to configure my own OpenAI-compatible LLM endpoint, so that I can use a remote provider or local small model.
25. As a self-hoster, I want model adapters around Stockfish and Maia, so that I can swap providers later.
26. As a self-hoster, I want the app to run locally, so that I can use the coach without relying on a Central Host.
27. As an operator, I want a Central Host deployment path, so that Players can use ChenChess without self-hosting.
28. As an operator, I want the Central Host frontend to be a Vite static build, so that we avoid unnecessary Next.js complexity.
29. As an operator, I want Stockfish packaged with the Rust backend for Central Host MVP, so that deployment stays simpler.
30. As an operator, I want Maia as a separate service for Central Host MVP, so that model inference can be scaled or swapped independently.
31. As a developer, I want Engine Analysis behind a Model Adapter, so that Stockfish details do not leak into the Game Review pipeline.
32. As a developer, I want Human Move Model output behind a Model Adapter, so that Maia details do not leak into the Game Review pipeline.
33. As a developer, I want a Rule Extractor module, so that chess facts are deterministic and testable before LLM prose.
34. As a developer, I want a Game Review Orchestrator, so that the full pipeline has one clear owner.
35. As a developer, I want Auth Token validation isolated, so that protected Review Engine endpoints can share one authorization path.

## Implementation Decisions

- The product context is **ChenChess** (formerly Personal Chess Coach).
- The player-facing output is a **Game Review**, not generic analysis.
- A **Game Review** contains pipeline-selected **Critical Moments**.
- A **Player-Selected Moment** can be reviewed on demand during the same session, but is not persisted in MVP.
- The MVP must include the full pipeline, not a fallback-only tracer bullet.
- The canonical pipeline is: Game plus Elo Profile into parallel Engine Analysis and Human Move Model, then Rule Extractor, then LLM Explainer, then Game Review.
- Stockfish UCI is the MVP Engine Analysis provider.
- Maia is the MVP Human Move Model provider.
- External chess engines and models sit behind **Model Adapters**.
- The Rule Extractor owns deterministic chess coaching facts and must not be implemented as prompt-only logic.
- The LLM Explainer uses an OpenAI-compatible API and is prose-only.
- Explanation Style is selected per Game Review as `simple`, `standard`, or `advanced`.
- Elo Profile is per-review for MVP and is not a persisted Player setting.
- Better Auth SSO via the Convex Better Auth component handles client sign-in.
- The Review Engine protects application endpoints by validating `Authorization: Bearer <jwt>`.
- The JWT `sub` claim is the canonical Player ID.
- `POST /api/analyze` must be auth-protected.
- React talks to the Review Engine for application APIs.
- The frontend remains React + Vite, not Next.js.
- Local self-hosting and Central Host deployment are both required.
- The Central Host follows a Railway-style multi-service deployment pattern.
- For Central Host MVP, Stockfish runs inside the Rust backend container and Maia runs as a separate service.
- The MVP does not persist Saved Games.
- The MVP does not persist Player-Selected Moment feedback.

## Testing Decisions

- Tests should verify external behavior and module contracts rather than internal implementation details.
- Auth Token Verification should be tested with valid, invalid, missing, and wrong-subject JWT cases.
- Game Import should be tested with Chess.com-style PGN, Lichess-style PGN, malformed PGN, and PGNs with missing metadata.
- Engine Analysis Adapter should be tested with a fake UCI engine and contract tests for Stockfish command/response handling.
- Human Move Model Adapter should be tested with a fake Maia service and contract tests for Elo-aware move probability responses.
- Rule Extractor should be heavily unit tested because it is the deterministic core that prevents LLM hallucination.
- LLM Explainer should be tested with fixed extracted facts and a fake OpenAI-compatible response.
- Game Review Orchestrator should be tested with fake adapters to confirm it selects Critical Moments and supports Player-Selected Moment on-demand review.
- React Review UI should be tested for protected flow, PGN import submission, Elo Profile input, Explanation Style selection, Critical Moment display, and Player-Selected Moment interaction.
- Deployment checks should verify the Vite build, Rust build, backend health, and expected service environment variables.

## Out of Scope

- Persisting Saved Games.
- Persisting Player-Selected Moment feedback.
- Persistent default Elo Profile.
- Collecting player age or birthdate.
- Next.js adoption.
- LLM-only chess analysis.
- Direct browser calls to Review Engine-protected resources without a Better Auth JWT.
- Treating Maia as the objective Engine Analysis provider.
- Full move-by-move Game Review for every move.

## Further Notes

- Railway's Rust React starter is useful as a deployment reference, but it assumes Next.js and Postgres; ChenChess should adapt the multi-service deployment pattern without adopting that frontend framework.
- Maia integration may require a local inference service, LCZero-compatible runtime, or another self-hosted wrapper. The adapter contract should be designed before binding the rest of the pipeline to a specific Maia runtime.
- OpenAI-compatible LLM providers may support different structured-output capabilities, so the existing `LLM_RESPONSE_FORMAT` compatibility setting should remain part of the system.
