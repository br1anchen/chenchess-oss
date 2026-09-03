---
name: chenchess-coach
description: Produces validated Game Reviews and grounded interactive Review Sessions through the local ChenChess CLI. Use when the Player directly asks to review a supported completed Chess.com Game, Lichess Game, pasted PGN, or local PGN file, or continues a ChenChess review with plan or Alternative Move questions.
---

# ChenChess

Use the installed `chenchess` CLI for every chess fact. The active agent is the Language Layer: write explanations from typed CLI facts, but never invoke another hosted model or make position-specific claims from general knowledge.

## Inputs

Accept one completed Game as a strict `https://www.chess.com/game/computer/<numeric-id>`, `https://www.chess.com/game/daily/<numeric-id>`, or `https://www.chess.com/game/live/<numeric-id>` URL, strict `https://lichess.org/<8-or-12-character-id>[/white|/black]` URL, pasted PGN, or local PGN path.

- A side-qualified Lichess URL selects that side. A bare URL requires `white` or `black`.
- A Chess.com Game URL requires `white` or `black`; it never selects a side from the URL.
- Pasted and local PGN require Review Side `white`, `black`, or `both` and Elo Profile 100 to 3500.
- If Lichess metadata cannot resolve Elo Profile, ask for one; never clamp it.
- Explanation Style is optional and defaults to `standard`; accept `simple`, `standard`, or `advanced`. Style changes wording only.

Do not automatically invoke this skill for a general chess question, screenshot, ongoing Game, or unsupported URL.

## Start from grounded facts

Create one private temporary directory. Start one long-lived `chenchess review-session --jsonl` process with `--command-fifo "<private-directory>/commands.fifo"` and keep its conversation, command identities, returned context, evidence, and branch values only in memory and that directory. Wait for the CLI-created FIFO, then write each complete JSON command line to it; never send Review Session commands through terminal stdin.

Import the Game through an `importGame` command. For a local PGN, pass only its path in `localPgnFile`; do not read the file into agent context. Follow [review-session.md](review-session.md) for exact command sequencing and state rules.

Keep the terminal `gameImported` event in the private temporary directory for validation. Use its `review` for Critical Moments, Position Views, evaluation history, and the Review Engine-selected Learning Plan. Before drafting the initial Game Review, send one `startReviewSession` with the opaque Game Import ID and require its matching completed event. Coach Skill requests the server-owned complete batch: every chronological `reviewMoments` entry must have `authoring.kind` `prepared`. A pending entry is not partial authority. Session start prepares objective Review Moment facts only and must not be treated as intent enrichment.

Open each Automatic Review Moment in Game order before drafting its explanation. For an unpublished Positive Highlight or Improvement Opportunity on the Review Side, the completed `reviewMomentOpened.authoringContext.intent` is the only intent-writing input: it contains static classification-aware instructions and, when providers succeed, exactly one selected four-ply Projected Plan SAN line plus one independent Objective Counterplay SAN line. Use only those lines, state exactly one explicitly uncertain hypothesis, and keep it inside the factual paragraph. When enrichment is absent, infer one reasonable possibility from the played move and grounded facts and keep it explicitly uncertain. Neutral and outside-Review-Side moments have no intent context and must contain no intent hypothesis. Never expose or reconstruct candidates, probabilities, scores, confidence, provider metadata, or selection traces. For every explained Critical Moment, copy the matching Review Engine-produced Position View and evaluation. Never reconstruct a board or calculate an evaluation.

Present Review Sessions chronologically: keep a persistent Game-order picker with move number, class, and validated evaluation change, then show only the selected Position Snapshot, coordinate-labelled board, comment, and that moment's replies. Switching moments retargets the active context without starting another Review Session. For zero Automatic moments, show the Game summary and timeline and offer a legal Player-selected moment.

Before drafting the Game Review, read [review-writing.md](review-writing.md) completely. Follow its causal explanation structure and use the validated SAN lines when they are present.

## Validate before presenting

Write the structured Game Review to a temporary JSON file and run:

```sh
chenchess validate-review --review-event-file "<game-import-event-json>" --review-start-event-file "<review-start-event-json>" --draft-file "<draft-json>"
```

Pass the single completed start event; its ordered `reviewMoments` array must be
the complete prepared batch, including an empty array for a zero-moment review.
The validator checks objective causal literals, Game order, invented chess
literals, and whether applicable explanations contain exactly one uncertain
intent hypothesis. It does not semantically reinterpret that hypothesis. Make
at most one structural repair after a validation failure. Never present an
invalid draft.

Consume `gameImported.review.learningPlan` and the active `reviewMoments[]` entry's `learningMaterial` directly, including for Player-selected moments returned by open or resume. Their tracks and canonical resource URLs are Review Engine-selected facts: never propose, reorder, replace, browse for, or author them. For Improvement support, name the returned track concept as the **Missing Idea**; for Reinforcement support, label it **Idea Reinforced**. Organize every nonempty track in this order: **Concept lesson** (exact `Learn` resources, or the grounded moment explanation when no exact Practice module exists), then **Pattern drilling** (exact `Drill` resources). Do not add a Real-game application stage: repeating the supporting Review Moment is not transfer practice, and generated transfer positions are deferred. Render every resource's exact title and canonical URL. Never expose rank, support count, or selection trace. Empty track collections render no learning section and must not trigger generic lesson or training-plan prose.

Submit one-shot Player Plan Evaluations and agent-authored Alternative Move Assessments through their Review Session commands. Present authored output only after the CLI returns a matching `completed` event for the same request and operation. A `rejected`, `unavailable`, `cancelled`, or `conflict` event is terminal and never authorizes presentation.

## Interactive coaching

Use `inspectPosition` before explaining a reviewed or explored Position. Copy the returned `textBoard`, `sideToMove`, and `evaluation`. Carry the complete returned `context` by value; never replace it with agent memory, a hypothesis reference, or a partial object.

Support natural Player plan discussion, optional one-shot Player Plan Evaluation, legal multi-ply Alternative Move Exploration, targeted coaching, cancellation, and steering as described in [review-session.md](review-session.md). Never turn ordinary plan discussion into an automatic evaluation or durable intent state.

When discussion of the selected Review Moment or an Alternative Move reaches a natural conclusion, ask whether the Player wants to select another Critical Moment. If the Player says yes, present a fresh chronological Critical Moment picker with the currently selected board Position again and let the Player choose; never rewrite an earlier moment card after conversation has continued.

## Cleanup

On success, failure, or cancellation, stop the JSONL process and remove the drafts, event logs, and temporary directory. Do not copy them into a repository or persist the Game Review or Review Session.

Use [manual-scenarios.md](manual-scenarios.md) for host verification.
