# Chessiro Game Review architecture

Research date: 2026-08-08

## Bottom line

Chessiro is not doing the whole review locally. Its current public client uses a hybrid pipeline:

1. Stockfish analysis, move classification, and training-theme selection run in the browser.
2. Coaching prose, follow-up conversation, and text-to-speech use same-origin server APIs.
3. Training recommendations come mainly from deterministic detectors and ranking rules, not from the LLM.

This is easier to ship than ChenChess's deterministic Decision Explanation pipeline. It produces a fast, compelling review without first building a large concept ontology or proof system. The tradeoff is that commentary and learning recommendations are parallel outputs rather than conclusions forced to agree through one auditable explanation object.

## Observed architecture

The product behavior is described in Chessiro's [Game Review article](https://chessiro.com/blog/inside-chessiro-game-review). The implementation findings come from its public production [review/coach](https://chessiro.com/_next/static/chunks/1130665006325c21.js), [focus-area](https://chessiro.com/_next/static/chunks/67e987ff2700068b.js), [training-selection](https://chessiro.com/_next/static/chunks/fd2b7508a4043448.js), and [Stockfish worker](https://chessiro.com/engines/stockfish-18-lite-worker.js) code inspected on the research date. Chunk hashes and server internals can change.

| Stage | Location | Result |
| --- | --- | --- |
| Stockfish analysis | Browser worker | Evaluations, best moves, principal variations |
| Move classification | Browser | Quality labels based on win-probability loss and position facts |
| Focus-area detection | Browser | Ranked tactical, positional, phase, and habit evidence |
| Recommended Training | Browser | At most four themes and deterministic training links |
| Coach comments and chat | Server API | Streamed prose and follow-up answers |
| Text-to-speech | Server API | Audio narration |

Chessiro identifies the engine as browser-side Stockfish 18.0.8 lite-single in its [engine attribution](https://chessiro.com/legal/engine-assets). Search depth, time, memory, and worker count adapt to device capacity. That is sensible for a cheap interactive preview, but different devices can produce different search evidence. Canonical learning records should therefore retain engine configuration or be revalidated at a trusted boundary.

## How comments and learning material are generated

After local analysis, the client sends game metadata plus per-ply SAN, FENs, evaluations, move quality, best move, principal variation, and optional clock data to `POST /api/ai-coach`. Server-sent events return comments keyed by ply. The client keeps partial successes, retries missing comments, and cleans and truncates the text. Its default model identifier is `gemini-3-flash-preview`, matching Google's [Gemini model code](https://ai.google.dev/gemini-api/docs/models/gemini-3-flash-preview), although client code cannot prove the server actually uses that model.

For some follow-up questions, the browser validates a referenced move, runs a small local MultiPV search, and sends the result as extra engine context. That is a useful targeted guardrail. There is no visible general verifier that checks whether every strategic sentence follows from the supplied line. Chessiro itself reports about 75–80% AI-coach correctness and says tactics fare better than deep strategy, endgames, and openings ([Best AI Chess Coach](https://chessiro.com/blog/best-ai-chess-coach), [Can AI Coaching Work?](https://chessiro.com/blog/can-ai-coaching-work)). It does not publish enough evaluation methodology to reproduce that figure.

Recommended Training follows a separate deterministic path. The browser replays the game, compares played moves with Stockfish choices, detects motifs and behaviors, then ranks, thresholds, deduplicates, and diversifies the evidence. Exact puzzle themes are preferred; broader and phase-level fallbacks map to existing training routes. Chessiro's article says definitions, visual examples, and topic practice exist, while full lessons are still forthcoming. Its related posts describe mistakes becoming puzzles and recurring patterns being resurfaced through spaced repetition ([Puzzles From Your Own Games](https://chessiro.com/blog/puzzles-from-your-own-games)).

The simplicity comes from this separation:

- engine facts feed hand-written theme detectors and training links;
- the same engine facts independently feed an LLM for friendly prose.

The bulk commentary request does not include the selected focus areas, and the public client shows no shared proof object tying comment, recommendation, and drill together.

## Existing components and services

- [Chessvia API](https://www.chessvia.ai/api) is the closest apparent hosted service: full-PGN analysis, move classifications, turning points, errors, and a `study_plan`. Its public page omits pricing, SLA, grounding method, and evaluation evidence, so it is suitable for a benchmark spike rather than immediate architectural dependence.
- [Stockfish](https://github.com/official-stockfish/Stockfish) and [stockfish.js](https://github.com/lichess-org/stockfish.js) provide search, not pedagogy.
- The [Lichess puzzle database](https://database.lichess.org/#puzzles) and [lichess-puzzler](https://github.com/ornicar/lichess-puzzler) are the strongest open references for extracting, validating, tagging, and rating tactical positions, but not a hosted arbitrary-PGN-to-curriculum API. See the existing [Lichess mapping note](./lichess-missed-motif-learning-mapping.md).
- [Concept-guided Chess Commentary](https://github.com/ml-postech/concept-guided-chess-commentary) ([paper](https://arxiv.org/abs/2410.20811)) combines expert-model concepts with an LLM. It supports expert-first grounding conceptually, but is a research pipeline rather than a production SDK.

I found no mature open-source drop-in library that turns arbitrary reviewed PGNs into both grounded coaching prose and durable learning plans.

## Recommendation for ChenChess

Keep the deterministic Decision Explanation and knowledge-graph pipeline as the semantic authority, but borrow Chessiro's product staging:

1. Offer fast local Stockfish analysis as an optional preview, with provenance.
2. Ship a narrow set of high-precision typed motifs and abstain outside it.
3. Select learning tracks and drills deterministically from validated proof paths.
4. Let an LLM render concise language only from those proof paths.
5. Connect review directly to practice and spaced recurrence.

This keeps Chessiro's easy, responsive loop while preserving ChenChess's advantage: comments, concepts, and exercises can all derive from the same checked evidence. The relevant decisions are [ADR 0038](../adr/0038-lead-chess-concept-learning-with-decision-explanations.md), [ADR 0036](../adr/0036-classify-learning-motifs-from-best-and-refutation-lines.md), and [ADR 0035](../adr/0035-select-learning-plans-in-the-game-review-engine.md).
