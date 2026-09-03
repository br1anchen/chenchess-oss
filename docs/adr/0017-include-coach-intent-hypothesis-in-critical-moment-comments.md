# ADR 0017: Include Coach Intent Hypothesis in Critical Moment Comments

## Status

Accepted in part. The evaluation-first opening, Critical-Moment-only scope, comment fact shape, and safe-rendering/publication rules are partially superseded by ADR 0019. The reproduction-manifest clause is superseded by ADR 0020. The Entry Critical Moment and chronological-session clauses are superseded by ADR 0021. ADR 0026 supersedes the remaining intent-selection, interaction, evidence, calibration, and eager-preparation decisions while preserving the deterministic fact boundary.

## Context

ADR 0016 kept move intent out of the initial Game Review while narrowing causal coaching facts to deterministic observations. That fact boundary is valid: neither Stockfish, Maia, nor the language layer can establish a Player's private purpose as a chess fact. The product conclusion does not follow from it, however. An explicitly uncertain Coach Intent Hypothesis can use those sources to begin a conversation without being presented as authoritative Move Intent.

The current evaluation corpus validates deterministic pipeline behavior and one recorded provider journey. It does not contain Player-authored Move Intent captured before hypothesis exposure, so it cannot measure intent-hypothesis accuracy or confirmation anchoring.

## Decision

Every prepared Review Moment may present a Review Moment Comment containing grounded chess facts and at most one explicitly uncertain Coach Intent Hypothesis. The pipeline chooses the highest-probability eligible Favorable Continuation for the Player's Elo Profile. When evidence is too weak or ambiguous, it emits a Coach Intent Abstention instead of forcing a hypothesis. The hypothesis or abstention is phrased inline with the position analysis; the typed state, rather than a separate prose question, enables the Player to confirm, correct, or provide their original Move Intent.

The Game Review contract represents intent as a tagged Critical Moment Intent State: hypothesis, abstention, or unavailability. A hypothesis includes stable evidence and Intent Selection Trace references. The frontend renders this typed state inline within one coherent Critical Moment Comment rather than relying on prose parsing or presenting a disconnected hypothesis card.

Intent selection and validation happen before prose generation. The Language Layer receives the validated Critical Moment Intent State together with the typed causal facts and authors one coherent comment. On the web this is the LLM Explainer; in the Coach Skill it is the active agent. It may translate that evidence into Player-appropriate language, but it cannot choose a different continuation, add another intent, convert abstention or unavailability into a hypothesis, or alter evidence references.

The Grounding Gate enforces the causal and intent boundaries before presentation. It requires the evaluation-first opening, validated played move, first refutation, first better move, complete supplied mechanism, reported evaluation literals, and grounded inline intent lead-in. It rejects output that assigns the hypothesis as authoritative Move Intent, introduces another intent, contradicts the tagged state, exposes evidence or trace identifiers, renders a separate intent section or confirmation question, or changes evidence and trace references. These are structured contract failures even though the gate does not attempt open-ended semantic chess judgment. Rejected prose never reaches the Player.

The pipeline may regenerate once after a grounding failure, using the same Explainer Candidate, generation contract, facts, and Critical Moment Intent State. It does not switch models or ask the failed prose to repair itself. If the second output fails, Safe Critical Moment Rendering deterministically combines the evaluation-first opening, inline uncertainty-aware intent wording, validated played move, refutation, better move, mechanism, and reported evaluations. The complete causal comment therefore remains available without presenting raw centipawn selection telemetry, trace identifiers, a separate intent section, or unvalidated language.

The initial comment uses Intent Selection Policy v1: a three-path global beam over four half-moves, representing two future Player moves; at least 10% captured probability mass; at least 2% leading-path joint probability; and a favorable candidate within the top two paths whose probability is at least 20% of the leader's. The highest-probability eligible favorable candidate grounds the sole hypothesis. Published thresholds and horizons are immutable; any later change introduces a new policy version for deliberate evaluation.

Before the first calibrated intent-policy release, implementation review found that the original 25% relative-probability floor suppressed plausible favorable runner-up continuations, including a 23.64% candidate in a real Player review. The v1 launch value is therefore 20%. This remains a ranking heuristic rather than a calibrated confidence claim.

Intent Selection Policy v1 gives intent preparation 15 seconds. Under ADR 0021, every Automatic Critical Moment shares the one absolute session-start deadline; a later Player-Selected Moment receives the same bound for its preparation. A timeout records Coach Intent Unavailability and returns the factual Review Moment Comment plus the request for Player Move Intent. Deployments cannot silently extend the deadline and hold the review open longer.

Intent evidence failure is isolated from Game Review availability. If Stockfish, Maia, the intent projection, or its deadline fails, the Critical Moment Comment still presents its validated chess facts and asks the Player what they intended. The artifact records Coach Intent Unavailability with operational provenance. It does not emit a Coach Intent Abstention, because abstention means complete evidence was valid but insufficient or ambiguous.

During initial preparation, the runtime may retry one transient technical failure internally only within the original 15-second deadline. Coach Intent Unavailability does not expose a Player-triggered retry or schedule a background or late retry. Once unavailability is emitted, the factual question is the terminal fallback. Confirming, correcting, providing intent, or skipping permanently closes hypothesis generation for that Review Moment interaction.

Confirming the hypothesis makes it authoritative Move Intent for the Player's stated purpose. Correcting it or providing another intent makes the Player's words authoritative instead. Either transition immediately starts a grounded Intent Assessment that separates plan fit, move alignment, and objective safety. Skip resolves the interaction without inventing Move Intent and produces no Intent Assessment.

If the Player's correction or supplied intent lacks the plan, target, or continuation needed for assessment, the coach asks at most one focused Intent Clarification before committing the Move Intent and generating the assessment. The clarification requests missing meaning; it does not propose wording or complete the plan on the Player's behalf.

If the response remains too vague after that clarification, the Player's words remain authoritative as their stated purpose but the coach records Intent Assessment Abstention. It explains that the available meaning cannot support plan-fit or move-alignment claims and continues without emitting a partial or invented Intent Assessment.

Session start prepares every Automatic Critical Moment and returns the complete set in Game order. The Coach Skill consumes that set before drafting the full review because its Review Validator requires a matching typed intent state for each explanation; interactive surfaces may defer prose authoring until presentation. A Player-Selected Moment enters the same workflow when it is opened. In every surface the hypothesis is not hidden behind a facts-first reveal gate, and it is never stored or described as Player-stated Move Intent unless the Player confirms or supplies it.

The first version communicates uncertainty through consistent inline wording such as "Your move ... was likely aiming for ..." It does not display a probability, confidence badge, categorical confidence label, or internal trace identifier. Without an Intent Calibration Set, those displays would imply a measured reliability that the product cannot support.

Each hypothesis or abstention retains an internal Intent Selection Trace in the Review Session Evidence Packet. The trace pins the Elo Profile, eligible candidates, evidence references, raw ranking signals, selected candidate, and abstention reason. This supports deterministic replay, debugging, and later evaluation without presenting those signals as calibrated confidence or Player-authored Move Intent.

When the Central Host retains a Review Snapshot, it stores the trace, generated comment, and direct typed replay inputs needed for evaluation, including the exact Stockfish build and configuration, Maia runtime and model, resolved Elo, intent-selection policy, Explainer Candidate, response schemas, generation settings, and code revision. These inputs are retained directly rather than wrapped in a product-facing reproduction manifest. A mutable provider alias or deployment name is not reproducibility evidence.

Coach Intent generation uses the provider's lowest supported randomness and a stable seed when available. The Player should receive a stable highest-ranked hypothesis for identical evidence. The Review Snapshot still retains the emitted Critical Moment Comment because model infrastructure and providers may not guarantee byte-identical output even under a pinned low-randomness contract.

First-version release is not blocked on Intent Hypothesis Precision or Intent Hypothesis Coverage. A defensible gate requires a held-out Intent Calibration Set of real Critical Moments paired with Player-authored Move Intent captured before hypothesis exposure, and that dataset does not exist. Post-hypothesis production confirmations do not substitute for it. Building the dataset and defining a promotion gate are deferred.

When accepted, this decision superseded only ADR 0016's statement that move intent stays out of the initial review. ADR 0016's deterministic fact boundary and semantic-grounding requirements remain in force; ADRs 0019–0021 subsequently refine this decision as recorded in its status.

## Consequences

The first version can use intent as an uncertain interaction opener, but it cannot claim empirically validated hypothesis accuracy or confidence. Product wording, contracts, and stored state must preserve the distinction among Critical Moment Intent State, Coach Intent Hypothesis, Coach Intent Abstention, Coach Intent Unavailability, Intent Selection Trace, and Player-authored Move Intent.

The existing deterministic evaluation corpus remains responsible for fact and pipeline regressions. A later accuracy claim or release gate requires a separately collected, consented, pre-exposure Intent Calibration Set with Player-separated development and held-out partitions.

Retaining production artifacts supports later policy analysis but does not justify silently retuning Intent Selection Policy v1. A candidate policy must be replayed and compared under an explicit version before adoption.
