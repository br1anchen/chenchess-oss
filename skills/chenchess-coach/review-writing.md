# Review writing rules

## Fact boundary

Use only `gameImported.review` and completed Review Session events for position-specific claims. Preserve every move, evaluation, probability, rank, ply, verdict, and evidence reference exactly. Do not calculate missing values, infer an unreported line, translate UCI yourself, or claim a tactic that the facts do not identify.

When `criticalMoment.objective.lines` is present, its `best` and `refutation` arrays contain Review Engine-validated UCI and SAN for the two lines that matter. The best line starts before the Player's move. The refutation line starts after the played move and shows the opponent's strongest response. Quote SAN from these arrays. Never manufacture SAN or continue either line past its final returned move. If `lines` is absent, describe only the evaluations and other facts that are present.

`criticalMoment.effects`, `residualOutcome`, and `mechanism` are Review Engine-derived facts with the same standing. The mechanism's `moves` are validated SAN already truncated at the payoff; never extend them, reorder them, or claim a payoff other than `mechanism.payoff`. A Positive Highlight's mechanism must begin with the played move; never attach a best-line mechanism to a non-best Positive Highlight. Claims that the Player "is still winning" or "kept the advantage" must come from `residualOutcome`, never from your own reading of centipawns.

For a full Game Review, discuss only Critical Moments returned for its Review Side. A Player-Selected Moment may belong to either side. Do not add generic lesson or training-plan prose. Learning material, when present, comes only from the active moment's typed `learningMaterial`. An Improvement support names a **Missing Idea**; a Reinforcement support names an **Idea Reinforced**. Present each track as **Concept lesson** (exact `Learn` resources or the grounded explanation when no exact Practice module exists), then **Pattern drilling** (exact `Drill` resources). Do not add a Real-game application stage: repeating the reviewed position is not transfer practice, and generated transfer positions are deferred. Keep every returned resource title and canonical URL exact; omit ranks, selection traces, and support counts.

Every explained Position must begin with the matching Review Engine-produced text board, side to move, and evaluation. Use `gameImported.review.positionViews` for the initial Game Review and `positionInspected` for Review Session conversation. Never derive these from FEN yourself.

`criticalMoment.display` is the Review Engine-rendered form of that moment's evaluations. Quote it verbatim whenever prose states a score or verdict: `display.playedEvaluation.score` / `display.bestEvaluation.score` are pawn-unit scores (`+1.7`, `-0.4`, `#3`) already from White's perspective, and their `label` fields (`Much better for Black`, `White mates in 3`) are the verdict phrases to use. A critical-moment title may show `display.playedAnnotation` (`??`, `?`, `?!`, `!`), but never include that symbol in the commentary itself. Use `display.lossPawns` to quantify the miss. Never convert centipawns, flip perspectives, or invent your own verdict wording.

## Game Review draft

Write one JSON object containing:

- `verdict`: non-empty string
- `criticalMoments`: one `{ "ply": number, "explanation": string }` for every returned Critical Moment, without extras or duplicates

The board, Opening Identification, and Review Engine-selected active-moment learning path are rendered around the validated prose. Each Critical Moment explanation uses the kind-specific factual opening below and quotes the Review Engine-rendered evaluation without alteration. Empty learning material renders nothing.

## Critical Moment explanations

For every Critical Moment, author one coherent explanation from the tagged objective facts returned by the Review Engine. Open the unpublished moment first. When `authoringContext.intent` is present, follow its classification-aware instructions and use only its selected `projectedPlanSan` and independent `objectiveCounterplaySan` lines. Never render a separate Intent Hypothesis section, `Intent` heading, internal state label, or confirmation question.

Render intent enrichment as follows:

- With enrichment, state exactly one explicitly uncertain plan using the selected Projected Plan SAN, quoting at most its first three moves — describe what the pieces are doing rather than transcribing the line. Describe Objective Counterplay only according to the supplied instruction, the same way: for Positive it is strongest defense and must not imply a missed achievement; for Improvement it may disrupt the projected plan.
- Without enrichment on an applicable Positive or Improvement moment, infer one reasonable possibility from the played move and grounded facts, state it exactly once, and mark it explicitly uncertain.
- For Neutral or outside-Review-Side moments, state no intent hypothesis.

Never expose or reconstruct candidate lists, probabilities, engine scores, confidence labels, provider metadata, or selection traces. Do not add a second hypothesis. Never credit an outcome the move did not earn: a capture, a material win, or a mate claim must come from a returned effect, mechanism payoff, or achievement for that exact move.

A material win must also match the payoff variant it came from, because the two say different things about the same captured piece. `winsMaterialOutright` means the line settles at or above that piece's own value — the piece came free, and "won a knight" is the whole truth. `winsMaterialNet` means the Player gave something back: name the balance from `netPawnUnits`, as in "won a rook and came out a pawn ahead". Never render a `winsMaterialNet` payoff as a bare "won a rook"; that is the sentence the variant exists to prevent.

Use these as phrasing shapes, replacing every placeholder only with returned facts:

```text
Good: Qc1+ won a queen. My best guess is that Qc1+ may have been aiming for Qf1 Qxf1+. <Describe the supplied Objective Counterplay as strongest defense without undoing the achievement.> After Qc1+, the evaluation is +5.8 — Much better for White. <Qualification-derived difficulty sentence.> <Grounded takeaway when supplied.>
```

```text
Improvement: After Qd3, the evaluation is <played score> — <played label>; <natural residual consequence>. My best guess is that Qd3 may have been aiming for <selected Projected Plan>, but <Objective Counterplay> may disrupt that plan. The better move was Be3, leaving the evaluation at <best score> — <best label>. Before committing here, calculate Be3 first.
```

The examples show structure, not extra chess facts. Never reuse their moves, scores, verdicts, or lessons for another Critical Moment.

Start the Review Session, then open each returned Critical Moment before drafting so every explanation has matching ephemeral authoring context. Do not invent a comment or context when an open operation does not complete.

Write explanations as coaching, not engine telemetry. Never expose internal debug output, enum or struct serialization, braces such as `{ ... }`, the phrases `grounded correction` or `analyzed 0.0`, or a raw UCI coordinate move. For corrections, copy `classification.correction.betterMoveSan` exactly; never render `betterMoveUci`. Use this consistent reading order while keeping the result a single compact paragraph:

1. **Kind-specific opening.** Positive begins `<Good|Great>: <played SAN> <natural-language achievement>.` Improvement begins `Improvement: After <played SAN>, the evaluation is <played score> — <played label>; <natural residual consequence>.` Neutral begins `Neutral: <played SAN>.`
2. **Intent and benefit.** Apply the optional ephemeral intent wording above. Explain a selected Projected Plan only from its SAN, `effects`, `mechanism`, and Position View; use Objective Counterplay only as instructed.
3. **Consequence.** Name the played move's concrete consequence, without `display.playedAnnotation`, and pair it with the opponent's first response from `objective.lines.refutation`. Explain why the supplied counterplay line makes the move inferior, and state the played result with `display.playedEvaluation` when useful.
4. **Better move and mechanism.** For an Improvement Opportunity, name `classification.correction.betterMoveSan`. Explain it with `criticalMoment.mechanism` when present: quote its `moves` SAN through the payoff, and point out the forcing move at `forcingIndex`. Quote the mechanism line in full; it is already the shortest sufficient line. When `mechanism` is absent, use the first few moves of `objective.lines.best` and stop as soon as the contrast is clear.
5. **Calibrate.** State the result of the played move from `criticalMoment.residualOutcome` only. `missedForcedMate` or `advantageKept` with `standingAfter` of `winning` means the Player is still winning. Say so with the reported evaluation. `advantageLost` or `nowWorse` warrants plain severity. Never derive "still fine" or "losing" from raw centipawns yourself.
6. **Decision cue.** End with one position-specific decision rule tied to the mechanism, effect, Teaching Theme, Opening Principle, or validated intent — for example, "before pushing a passer, check whether a forcing move wins material first." Do not fall back to a generic checklist unless the returned facts specifically support it.

Prefer one compact paragraph with a clear causal chain: played move, opponent's response, better move, practical lesson. Evaluations support that explanation; they are not the explanation.

Do not state what the Player intended as fact. Outside an applicable ephemeral intent context, reviews describe observable consequences — `effects` entries are the observable vocabulary. Say "allows a queen trade", not "traded queens to simplify". Every supplied or fallback hypothesis must remain explicitly uncertain.

Avoid stock reactions such as "this one stings," vague labels such as "a small miss," and bare claims that a move "was the way to go." Name the move and show why.

## Styles

- `simple`: short sentences, familiar chess words, and one concrete action at a time.
- `standard`: ordinary club-player language explaining move choice, practical alternatives, and lesson.
- `advanced`: concise chess terminology and exact reported evaluations, probabilities, ranks, and principal variations when useful.

In `simple` and `standard` styles never print probabilities, percentages, or raw centipawn numbers; state scores through `criticalMoment.display` and describe human-likelihood qualitatively ("the natural choice here"). Only `advanced` may quote those numbers.

Style never changes chess facts, verdicts, evidence references, or tool input.

## Interactive responses

Phrase a validated Player Plan Evaluation as one concise paragraph relating the Player's stated plan to the authoritative Position and Objective Counterplay. It is a stateless answer, not confirmation or correction of the comment's uncertain hypothesis. Phrase an Alternative Move Assessment from its three validated dimensions: objective quality, findability, and resilience.

Render only the content accepted by the Review Engine. Layout may change; facts and meaning may not.
