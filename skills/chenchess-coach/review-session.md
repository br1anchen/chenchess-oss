# Review Session JSONL workflow

Keep one `chenchess review-session --jsonl --command-fifo "<private-directory>/commands.fifo"` process alive for the Review Session. Wait for the CLI-created FIFO, write one compact JSON command envelope per line to that FIFO, and consume events until that operation reaches exactly one terminal event. Never send commands through terminal stdin: canonical PTYs can discard a JSONL line before ChenChess receives it.

Every envelope contains fresh opaque identities and the target surface:

```json
{
  "requestId": "request:<fresh>",
  "operationId": "operation:<fresh>",
  "surface": "coachSkill",
  "command": {}
}
```

Never reuse a request, operation, Coach Turn, or publication-fence identity for new work. Never calculate a Position reference, evidence identity, packet digest, branch reference, or append receipt.

## Import

Use one source variant:

- `{"kind":"chessComUrl","url":"<strict-computer-or-live-game-url>"}`
- `{"kind":"lichessUrl","url":"<strict-url>"}`
- `{"kind":"pastedPgn","pgn":"<already-pasted-text>"}`
- `{"kind":"localPgnFile","path":"<path>"}`

Only side-qualified Lichess URLs use `{"kind":"fromQualifiedUrl"}`. Chess.com Game URLs use `{"kind":"selected","reviewSide":"white|black"}`. Coach Skill pasted/local imports use `{"kind":"selected","reviewSide":"white|black|both"}`. Elo is `{"kind":"fromImportedMetadata"}` or `{"kind":"playerProvided","rating":1246}`.

Retain the complete `gameImported.review`. The full-game graph uses only `review.evaluationTimeline`; Practice selection uses its corresponding `review` field. If import requests Review Side or Elo, correct the input and start a fresh import operation.

Retain `gameImported.timing`. If the Player asks why a review took a long time, surface `timing.totalPipelineMilliseconds` and use the Engine Analysis and Human Move Model call summaries to explain the local fact pipeline. In the pinned local runtime, these measure Stockfish and Maia respectively. Keep runtime startup separate, and do not attribute time above these measurements to a provider.

## Start and inspect

Send `startReviewSession` with only `{"kind":"startReviewSession","gameImportId":"<returned-id>"}`. Only a matching completed `reviewSessionStarted` event authorizes use of its `sessionId` and chronological `reviewMoments`. Coach Skill receives one server-owned complete batch: require every entry's `authoring.kind` to be `prepared`, use its `authoring.core` as the objective Review Moment authority, and use that same entry's `learningMaterial` as its display authority. For `reviewSessionResumed`, locate the active entry by `reviewMoment.momentId`. `reviewMomentOpened` ships only the moment it opened: its `reviewMoment` is the objective authority and its `criticalMoment` is the display authority, including for Player-selected local material. Keep the session's batch from start or resume; an open never restates it. Never orchestrate preparation one moment at a time, and never treat a pending entry as authoring authority. Session start performs no Maia or Stockfish intent enrichment.

Before authoring an unpublished comment, open that Review Moment with a fresh publication fence. Positive and Improvement moments on the Review Side receive optional `authoringContext.intent`: follow its static instructions and use only `enrichment.projectedPlanSan` and `enrichment.objectiveCounterplaySan` when present. Write exactly one explicitly uncertain hypothesis. If enrichment is absent, infer one reasonable possibility from the played move and grounded facts without presenting it as fact. Neutral and outside-Review-Side moments receive no intent and no hypothesis. Positive opens with its grade, played move, and concrete achievement, then gives qualification-derived difficulty and a reusable takeaway only when teaching facts support one; Improvement opens with its outcome and consequence, preserves the supplied correction, and ends with its supplied decision cue; Neutral contains only its closed reasons and verified observation. Preserve `?`, `??`, `?!`, and `!` only in a critical-moment title, never in commentary. Never expose provider selection details or render a separate intent section or confirmation question.

## Chronological presentation

Keep a persistent Game-order picker in every Review Session response. List each prepared or opened Review Moment in game order with its move number, explicit class (`Positive Highlight`, `Improvement Opportunity`, `Neutral`, or `Player-selected`), and the validated evaluation change when the matching Game Review display provides one. A class name and glyph must be present; color is never the only distinction.

Only one Review Moment is active at a time. After the picker, render that selected moment's Review Engine-produced Position Snapshot: the coordinate-labelled text board, side to move, evaluation, one validated comment, and only the replies belonging to that moment. When switching moments, replace this active section rather than repeating other moment conversations or creating another Review Session. State the concise game recall since the preceding selected moment, or that this is the first Review Moment.

Keep Player plan discussion in the normal conversational reply flow. Do not introduce an intent card, confirmation, correction, skip, clarification, assessment controls, or a second session merely because the Player chose a different moment. With zero Automatic moments, present the Game summary and evaluation timeline, then allow the Player to open any legal Player-selected moment.

Before explaining a Position, send:

```json
{
  "kind": "inspectPosition",
  "sessionId": "<session>",
  "target": { "kind": "reviewedMove" }
}
```

For an explored node use `{"kind":"alternativeMove","alternativeMoveId":"<returned-id>"}`. The completed inspection is the current authority for `context`, `evidencePacket`, `textBoard`, `sideToMove`, and `evaluation`. Inspect at most twice during one Language Layer turn.

## Player Plan Evaluation

Continue conversationally when the Player explains their plan. Call Player Plan Evaluation only when an engine-backed comparison would materially improve the answer; never invoke it automatically for every plan comment.

Send `evaluatePlayerPlan` with `request.kind` of `prepare`, the current `sessionId`, and `reviewMomentId`. The completed `playerPlanEvaluationPrepared` context is the sole chess authority. Interpret the Player's current words against those facts in memory, author one concise paragraph using only supplied SAN, squares, and evaluations, then send `evaluatePlayerPlan` again with `request.kind` of `admit`, the returned `factsRef`, and that paragraph. Present only `playerPlanEvaluated.text`. A rejection or unavailable outcome ends the one-shot operation: never ask a tool-driven clarification or create intent state. Do not place the Player wording in either command, a draft file, logs, or retained artifacts.

## Review Moment comment admission

Before presentation, submit the complete ordered Draft Game Review to the Coach Skill validator. The validator applies the same kind-aware objective-facts policy as the Web grounding gate: it rejects missing, extra, cross-kind, altered, invented chess literals, authoritative-intent, internal-reference, and multi-paragraph prose. It checks the presence and uncertainty shape of an applicable hypothesis but does not attempt semantic comparison. A rejected draft never reaches the Player. The Web gate retries the identical tagged facts, optional ephemeral intent context, and generation contract once; a second rejection renders the complete kind-specific safe paragraph instead. Malformed classification facts fail closed and are never safe-rendered.

## Alternative Moves

Submit `exploreAlternativeMove` with a returned root or branch parent, matching source Position reference, explicit SAN or UCI move input, the latest inspected packet digest, and a fresh publication fence. Never silently repair ambiguous notation.

Only `alternativeMoveEvaluated` commits a node. Failure, timeout, conflict, or cancellation commits nothing. Inspect the returned Alternative Move before explaining it. Use its offered strongest reply only after the Player accepts it; it is not committed automatically.

## Agent-authored targeted coaching

Start from the complete context returned by inspecting the target Alternative Move. Copy it by value and replace only `coachTurnId` with the fresh ID carried by `startCoachTurn`. Include the Player message, fresh publication fence, and `priorTurn` (`none`, `steers`, or `retriesUnavailable`).

On the Coach Skill surface, success returns `coachTurnPrepared`, not prose. Author an `AlternativeMoveAssessment` only from `facts` and cite exactly the evidence IDs listed for each dimension:

- objective quality: target branch, source engine, resulting engine;
- findability: target branch, source human model;
- resilience: target branch, resulting engine, resulting human model.

Submit `publishCoachTurn` with the authored assessment and the prepared evidence segment copied unchanged. Present only `coachTurnCompleted`.

## Cancellation and steering

Cancel active work with the exact target `operationId`, `sessionId`, and publication fence. Cancellation is idempotent; late success must not be presented.

For steering, cancel or supersede the active Coach Turn, create a fresh Coach Turn ID and fence, and use `priorTurn.steers` naming the old Coach Turn. Preserve the inspected target context unless the Player explicitly attached a different Alternative Move.

Treat `rejected`, `unavailable`, `cancelled`, and `conflict` as terminal typed outcomes. Follow their recovery value; never retry provider or semantic failure automatically.
