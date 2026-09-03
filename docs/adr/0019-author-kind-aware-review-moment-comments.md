# ADR 0019: Author and Publish Kind-Aware Review Moment Comments

## Status

Accepted in part. The requirement to return a Review Reproduction Manifest from product publication is superseded by ADR 0020. Review Session preparation and ordering are refined by ADR 0021. ADR 0026 supersedes typed Review Moment Intent State, intent ledger claims, semantic hypothesis matching, and intent-resolution controls. Kind-aware facts and authoring, objective Grounding Gate checks, safe rendering, and atomic publication remain in force.

## Context

The original Critical Moment Comment contract was mistake-oriented. It assumed an evaluation-first explanation, a better move when available, and one causal-explanation claim. Adding Positive Highlights makes that shape invalid: praise must explain a verified achievement without inventing a correction, while an Improvement Opportunity must identify a grounded correction rather than merely attaching negative wording to a move.

A Player may also open a legal move that the automatic selector did not choose. Such a Player-Selected Moment still needs an honest response even when it qualifies as neither Critical Moment kind. It must not be promoted to a third Critical Moment kind merely so the Language Layer has something to say.

The web application, installed Coach Skill, and Coach App use different Language Layers. Sharing prompt wording is therefore insufficient. They need one canonical fact boundary, intent model, grounding contract, failure policy, and publication rule even though their generated prose need not be byte-identical.

The Coach App adds a particular trust boundary. Its host model authors prose outside the Review Engine process. A valid transport or Review Session identifier establishes context and freshness, but it does not establish that the submitted prose preserved the authoritative chess facts.

## Decision

### Review Moment classification and comment scope

Review Moment Comment is the outer Player-facing concept. A Critical Moment Comment is its Positive Highlight or Improvement Opportunity subtype. A neutral Player-Selected Moment receives a Neutral Review Moment Comment through the same authoring path but does not become a Critical Moment.

Every Player-Selected Moment is deterministically classified as a Positive Highlight, Improvement Opportunity, or neutral. Neutral is an internal no-Critical-Moment result, is never admitted by automatic selection, and carries one or more closed reasons:

- `mechanicallyForced`;
- `soundWithoutConcreteAchievement`;
- `belowImprovementThreshold`;
- `nonInstructionalTerminalOutcome`.

The canonical result retains every applicable reason. Player-facing prose uses the most informative concise explanation and does not expose classifier thresholds, raw probabilities, or a list of failed gates. Missing, contradictory, or invalid evidence is an error rather than a neutral result.

### Canonical fact variants

Review Moment Comment authoring receives one tagged Review Moment Comment Facts variant rather than a flat structure containing optional fields from every kind:

```text
ReviewMomentCommentFacts
├─ PositiveHighlight
│  ├─ played move and played-move outcome
│  ├─ grade
│  ├─ Positive Highlight Qualification
│  ├─ concrete achievement references
│  └─ optional supported takeaway
├─ ImprovementOpportunity
│  ├─ played move and played-move outcome
│  ├─ concrete consequence
│  ├─ Improvement Correction
│  │  ├─ distinct legal better move
│  │  ├─ improved analyzed or avoided terminal outcome
│  │  ├─ optional validated first refutation
│  │  └─ optional validated Tactical Mechanism
│  └─ required reusable decision cue
└─ Neutral
   ├─ played move and played-move outcome
   ├─ non-empty Neutral Review Reasons
   └─ verified neutral observations
```

Fields from another variant are invalid rather than ignored. An analyzed outcome carries the required evaluation literals. A terminal outcome carries its verified board-terminal result and forbids a post-move evaluation.

A Positive Highlight remains qualified without a Teaching Theme, Opening Principle, Practice Recommendation, or other currently supported lesson. Qualification continues to require objective soundness, a concrete achievement, and objective-excellence or Elo-relative-achievement evidence. The comment includes a reusable takeaway only when deterministic teaching support exists; otherwise it omits the takeaway rather than emitting generic advice.

Positive difficulty wording is derived from Positive Highlight Qualification instead of a duplicate canonical field. Objective-only qualification may support precision or conversion wording but not a claim of human rarity. Elo-relative qualification may support `notable` or `strong` wording at the resolved Elo Profile. When both groups exist, the comment may express both. Raw move rank and probabilities remain evidence and need not appear in prose.

Every Improvement Opportunity requires an Improvement Correction: a legal better move distinct from the played move plus evidence that it improves the analyzed outcome or avoids the terminal outcome. A first refutation is allowed only when a validated post-move line supplies it. A Tactical Mechanism is allowed only when the corresponding verified mechanism exists. Terminal corrections contain neither a refutation nor a post-move evaluation. Without a grounded better move, the Review Moment cannot be classified as an Improvement Opportunity.

Every Improvement Opportunity comment ends with a concrete reusable decision cue derived from its Improvement Correction. A Teaching Theme, Opening Principle, or Practice Recommendation may enrich the comment but is not another qualification requirement.

### Intent and prose

Review Moment Intent State replaces the Critical-Moment-only name. It is classification-independent and carries exactly one of:

- Coach Intent Hypothesis;
- Coach Intent Abstention;
- Coach Intent Unavailability;
- intent not applicable because the move is outside the Review Side.

The existing evidence, uncertainty, timeout, and Player-response rules for hypotheses, abstention, and unavailability remain in force. ADR 0021 replaces Player-triggered retry with at most one internal retry inside the original deadline. Intent never establishes grade, achievement, correction, or Critical Moment classification.

For a Player-Selected Moment outside the Review Side, intent is not applicable. The comment identifies the mover by color and offers no intent hypothesis, confirmation control, or Move Intent invitation. When Review Side is both, mover-color wording is used for both sides rather than calling both movers the Player.

The Language Layer authors one coherent paragraph, not separately assembled sentence slots. The opening depends on the facts variant:

- Positive Highlight: grade, played move, and concrete achievement;
- Improvement Opportunity: evaluation or terminal result and concrete consequence;
- neutral: concise neutral verdict.

The inline Review Moment Intent State follows that factual opening. An analyzed comment preserves required evaluation literals somewhere in the paragraph; a terminal comment uses only its verified terminal outcome.

### Kind-aware Grounding Ledger and Grounding Gate

The Grounding Ledger contains the exact claim set required by the active facts variant:

- common: played move and played-move outcome;
- Positive Highlight: grade, achievement, qualification-derived difficulty, and an optional supported takeaway;
- Improvement Opportunity: consequence, better move, optional validated refutation or mechanism, and required decision cue;
- neutral: Neutral Review Reasons and verified observations;
- intent: the separate claims required by Review Moment Intent State.

The previous generic `selectionReason` and `causalExplanation` claims are removed.

The Grounding Gate deterministically rejects a draft that has a missing, extra, or cross-kind ledger claim; an unknown fact, evidence, or trace reference; altered required SAN, evaluation, grade, mechanism, or terminal literals; an unknown chess move or assertion; invalid intent certainty; a second intent; authoritative attribution of private Move Intent; exposed internal references or headings; or more than one paragraph. It also rejects prose whose exact intent presentation contradicts the typed state.

This gate proves structural and literal grounding. It does not perform open-ended chess interpretation or decide whether prose is insightful. Semantic explanation quality remains the responsibility of evaluation, calibrated judging, and Human Audit.

After the first grounding failure, the same Language Layer may author one new draft from identical facts, intent, and generation contract. A second failure invokes Safe Review Moment Rendering. The renderer has complete fixed templates for Positive Highlight, Improvement Opportunity, and neutral variants and fixed intent wording. It never degrades to a played-move-only comment. Invalid classification facts fail instead of rendering.

### Surface publication boundary

Validation is required because Language Layer prose is nondeterministic and untrusted, not because review history is durable. Review Session lookup establishes which facts and intent are current; grounding validation establishes whether the submitted payload is admissible.

The web path applies the Grounding Gate internally. The Coach Skill applies the same contract through its Review Validator. A Coach App host model must submit its draft through a central, atomic Review Moment Comment Publication operation before the inline workspace treats it as canonical.

The Coach MCP interface therefore has twelve tools. It adds model-and-app-visible `open_review_moment` and `publish_review_moment_comment` operations, and ADR 0021 makes `start_review_session` model-and-app visible so the host Language Layer can consume the prepared automatic set. The existing `publish_coach_turn` remains dedicated to interactive Alternative Move assessment; it is not widened into a union of unrelated publication payloads. There is no separate validation-only tool that could validate one draft and later display another.

Publication identifies the active Review Session and Review Moment and submits the draft text, Grounding Ledger, and publication fence. The Review Engine resolves the authoritative facts and intent from transient Review Session state and its facts registry, rejects stale publication, applies the kind-aware Grounding Gate, and returns only the accepted or safely rendered canonical comment as product state. Authoring provenance remains internal and may be retained inside a Central Host Review Snapshot. Host conversation prose that has not passed publication is noncanonical and is never rendered by the workspace as the official Review Moment Comment.

The Review Engine's Review Session state is transient domain state. It is distinct from the MCP transport session and live-operation capacity tracking and does not create durable review history. A Review Session identifier selects this authority; it does not replace payload validation.

Publication fails closed if the transient Review Session has expired. A URL-imported Game may be silently re-imported and a new Review Session started. A pasted PGN must be requested again. A client-carried fact packet cannot restore publication authority unless the server first reconstructs and verifies the authoritative review context.

## Consequences

Rust and generated contracts need tagged Positive Highlight, Improvement Opportunity, and neutral comment fact variants, the renamed and expanded Review Moment Intent State, closed ledger claims, and kind-aware validation and rendering.

Existing mistake-only comment code and fixtures cannot be extended safely by adding optional positive fields. The authoring input, validator, safe renderer, retained evaluation inputs, and surface adapters must migrate together.

The three delivery surfaces share admissibility semantics, not necessarily identical prose. Every canonical emitted comment is bound internally to its facts, intent, evidence, generation contract, validation outcome, and publication fence; product operations return the comment rather than that provenance bundle.

The Coach MCP interface gains two public operations and makes session start model-visible, superseding the fixed ten-tool and no-new-wire-vocabulary conclusions in the earlier Coach MCP tool-interface design. Cross-host contract and recovery documentation must be reconciled before implementation.

This decision partially supersedes ADR 0017's evaluation-first opening, Critical Moment Intent State name and scope, mistake-shaped Grounding Ledger, and Safe Critical Moment Rendering contract. ADR 0021 additionally supersedes ADR 0017's Entry Critical Moment, lazy later-moment initialization, and Player-triggered retry clauses. ADR 0017's remaining intent-selection, uncertainty, response, evidence, and calibration decisions remain in force.
