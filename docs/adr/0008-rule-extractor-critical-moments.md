# ADR 0008: Deterministic Critical Moment selection

## Status

Accepted.

## Decision

The Rule Extractor consumes a whole imported Game plus provider-neutral evidence for every played move. Each evidence item contains Engine Analysis before the move, either a post-move evaluation or an explicit terminal outcome, and Human Move Model candidates before the move. Missing, duplicate, or unknown ply evidence is an error; the extractor never presents a partial Game Review as complete. Explicit terminal evidence lets a checkmating final move complete the Game without inventing a normal engine `bestmove` for a position with no legal moves.

Stockfish evaluations use the side-to-move perspective. The extractor negates the post-move evaluation back to the mover's perspective before comparing it with objective best play. It records both values, best move, played UCI move, principal variation, centipawn loss when applicable, and the Human Move Model's top and played-move probabilities. This keeps objective and human-likely play explicit rather than asking the LLM to infer chess facts.

Automatic selection separates a versioned Selector Policy from versioned Selector Weights. Published versions are immutable. Any value change creates a new named version; runtime configuration cannot alter a published version.

### Selector Policy v1

An ordinary candidate must be a legal move with complete, valid evidence, belong to the requested Review Side, have an analyzed non-terminal result, differ from objective best play, and meet its Elo loss floor. Beginner, intermediate, and advanced floors are 150, 100, and 70 centipawns respectively. Selector Weights cannot bypass these gates.

Forced-mate deterioration is always eligible and ranks ahead of other candidates. The selector then uses the shared kind bands: Great Positive Highlight 900, advantage-lost Improvement 830, now-worse Improvement 730, Good Positive Highlight 650, advantage-reduced Improvement 630, and standing-kept Improvement 480. A quantized `0..99` evidence strength, derived from kind-specific grounded facts, breaks ties within each band; exact utility ties choose the earlier ordered ply list.

The adaptive target is `clamp(2 + ceil(gamePlies / 18), 3, 8)`, using the complete imported Game length regardless of Review Side. It is a ceiling, not a coverage obligation. The hard maximum of ten remains a separate fail-closed invariant. Selection first maximizes count up to the adaptive target, then maximizes total teaching priority less diversity penalties. If a qualifying Positive Highlight survives episode collapse, every selected subset contains at least one; that reservation consumes an in-target slot and never manufactures or overflows a moment.

The selector applies no ply-distance or adjacency exclusion. It applies 80-point soft penalties, never hard exclusions, for each candidate beyond `max(2, ceil(2 * target / 3))` from one category or Game phase. With Review Side `both`, it similarly applies penalties beyond `max(2, ceil(3 * target / 4))` from either side. Related forced decisions and continuations are collapsed into one Coaching Episode: the earliest meaningful decision is retained; a final payoff survives only when it independently qualifies; an episode with no representative is suppressed. Unrelated adjacent and cross-kind moments remain peers.

Game phase comes from the legal pre-move Position, not Stockfish or Maia metadata. Material phase units are `knights + bishops + 2 * rooks + 4 * queens` across both sides. A Position is opening through move 12 while at least 18 units remain, endgame when eight or fewer remain, and middlegame otherwise.

### Selector Weights v1

Selector Weights owns only the `0..99` evidence strength inside the Policy's fixed kind bands. Improvement strength is derived from the objective loss, Human Move Model likelihood gap, and played-move rank. Positive strength is derived from grade, qualification reasons, concrete achievements, and Elo-relative difficulty. Forced-mate deterioration receives the maximum strength. Tactical category and Game phase have zero direct priority weight; they participate only in the soft diversity policy.

The implementation quantizes kind-specific evidence strength to integer `0..99` before comparison. It finds the highest-utility subset under the policy instead of greedily accepting candidates. Exact set ties choose the earlier ordered ply list. Selected facts are returned in Game order; priority is retained only in the internal Selector Trace.

### Versioned evidence

Review Feedback Reports embed the complete Selector Policy and Selector Weights objects beside their immutable version names. Frozen Reproduction selects the algorithm by selector version, verifies that embedded values exactly match the named published versions, then replays them. A value/version mismatch is invalid evidence rather than selector drift.

The selector trace records the complete candidate pool and gates, Game length, adaptive target, hard maximum, episode outcome, Positive reservation, kind band, evidence strength, diversity outcome, priority rank, final Game-order position, and selected state. It is evaluation evidence only and never becomes a Player-facing ranking payload. A valid report may disagree with a hard gate. Such feedback remains policy-disagreement evidence, but Selector Weight Candidates cannot satisfy it by bypassing the policy.

This production v1 policy intentionally supersedes the earlier five-moment compatibility target. The implementation updates automatic-selection baselines deliberately rather than preserving the five-moment result behind a runtime switch. No published Review Feedback Report depends on the pre-policy behavior.

Tactical classification is conservative: a fact is tactical only when the played SAN is forcing (capture, check, mate, or promotion) or a mate score changes. Other selected facts are positional. Later board-aware rules may add named motifs without changing the provider-neutral evidence contract.

## Consequences

Selection is deterministic, bounded, Elo-aware, independently testable, and reproducible from a Review Feedback Report. The Game Review Orchestrator remains responsible for obtaining complete per-ply evidence and the LLM Explainer remains responsible only for wording these facts. Policy and weight changes require new immutable versions, focused selector tests, and deliberate promotion evidence.
