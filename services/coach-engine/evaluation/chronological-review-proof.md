# Chronological Review Implementation Evidence

This is the named requirement matrix and ordinary implementation evidence for
GitHub issue #117. It proves the supported local and shared-contract surfaces;
it is not a certificate, release manifest, schema-version layer, or Positive
Highlight quality benchmark.

The canonical automated case is the recorded `Synthet1` Game reviewed as Black.
It contains Positive Highlights and Improvement Opportunities in one
chronological sequence.

## Chronological review requirement matrix

| Accepted hard rule | Passing automated case | Failing or omission case |
| --- | --- | --- |
| Web, Coach Skill, and Coach App expose exactly the same order, kind, grade, qualification, outcome, category, provenance, Intent State, Practice selection, and admission facts. | `canonical_mixed_review_is_exact_across_web_coach_skill_and_coach_app` asserts the complete typed projection at plies 10, 22, 26, 34, 52, 72, and 78. | `rejects_a_supplied_positive_grade_that_disagrees_with_qualification`, `rejects_terminal_outcome_payloads_that_mix_in_analyzed_fields`, and `tagged_boundary_rejects_cross_kind_and_malformed_classification_facts` reject internally inconsistent projections. |
| The shared Coach App command and publication seam admits every canonical comment without a deployed host. | `coach_app_admits_every_canonical_mixed_comment_at_the_shared_publication_seam` and `coach_app_publication_is_authorized_fenced_and_grounded_at_the_shared_seam`. | The latter case also rejects the wrong principal, wrong surface, stale fence, mismatched intent authority, mismatched Review Moment, and ungrounded comment. |
| Web keeps the mixed review in Game order, reopens earlier moments, preserves per-moment interaction state, and exposes the exact grounded Practice Recommendation. | `keeps a mixed grounded review in Game order through navigation and intent correction`; `canonical_mixed_review_produces_the_exact_improvement_practice_recommendation`. | `invalid_identity_url_or_fact_reference_is_omitted`, `canonical_game_facts_allow_no_recommendation`, and `a superseded command cannot publish after its replacement has already finished`. |
| Coach Skill treats the persistent picker, Position Snapshot, ordered comments, Intent State, and Practice proposal as typed CLI authority. | `chronological_review_moments_and_typed_intent_state_are_cli_authoritative`, `every_explained_position_uses_rust_board_side_and_evaluation`, `practice_selection_is_grounded_and_none_is_omitted`, `one_atomic_start_supplies_intent_authority_for_every_chronological_moment`, and `validators_consume_the_game_import_event_from_review_session`. | `authored_outputs_are_validated_before_presentation`, `typed_intent_interaction_rules_match_the_shared_contract`, `start_authority_must_belong_to_the_imported_review`, and `skill_keeps_review_session_ephemeral_and_cleans_up`. |
| A `gameImported` completion with measured timing remains decodable by the web contract. | `decodes game-import timing represented by the uint64 contract format`. | `rejects undecodable and out-of-order transport data without exposing it`. |
| Gotham remains implementation and commentary evidence; any crash, nondeterminism, invalid classification, incorrect order, contract failure, or inadmissible comment blocks acceptance. | `gotham status` reports all 40 episodes complete in fetch, reconstruct, review, and compare with zero failed or pending phases. The accepted corpus contains 142 Games and 749 moments: 685 Positive, 64 Improvement, and two zero-moment Games. | Gotham has no aggregate score or quality threshold. Any listed structural failure is a hard failure regardless of the completed counts. |
| Existing CI and release proof remain the release gate. | `./tooling/nix-develop .#vanilla --command bun run release:proof` is the single blocking local command recorded below. | Any non-zero check blocks issue closure; no substitute certificate, manifest, schema layer, or quality benchmark is accepted. |
| Live ChatGPT and Claude verification is not part of this implementation gate. | Local web, Coach Skill, and shared Coach App contracts cover the currently available surfaces. | Live ChatGPT and Claude journeys belong to the separate cross-host staging prototype and do not block issue #117. |

## Automated variant matrix

The following variants remain automated and are not inferred from the manual
journeys.

| Variant | Passing case | Failing or boundary case |
| --- | --- | --- |
| Zero moment | `opens a legal Player-selected Moment from a zero-moment review`; `zero_moment_start_supplies_no_intent_authority`. | `zero_moment_start_authority_must_belong_to_the_game_import` rejects an empty start event borrowed from another import; `canonical_game_facts_allow_no_recommendation` proves explicit Practice omission. |
| Terminal | `selected_terminal_move_reports_missing_post_move_evaluation_without_inventing_one`; `all_intent_states_and_terminal_outcomes_remain_one_paragraph_grounded_renderings` | `rejects_terminal_outcome_payloads_that_mix_in_analyzed_fields`; `mate_comparisons_are_structured_and_terminal_nodes_cannot_extend`. |
| Forced sequence | `retains_earliest_decision_and_suppresses_forced_only_episode` | The forced-only episode is excluded while its earliest decision boundary remains eligible. |
| Threshold | `classifies_an_objectively_sound_concrete_achievement_as_a_good_positive_highlight`; `derives_great_only_when_objective_and_strong_elo_evidence_agree` | `rejects_a_supplied_positive_grade_that_disagrees_with_qualification`. |
| Retry | `one_failure_retries_byte_identical_tagged_input_then_admits_the_same_grounded_draft` | `unavailable_retry_uses_a_new_identity_and_keeps_prior_success_immutable`. |
| Fallback | `keeps factual coaching available when intent preparation is unavailable` | `hosted_admission_rejects_bad_ledgers_and_never_returns_unpublished_prose`. |
| Stale publication | `a superseded command cannot publish after its replacement has already finished` | `coach_app_publication_is_authorized_fenced_and_grounded_at_the_shared_seam` rejects stale and mismatched fences. |
| Invalid contract | `malformed_contract_and_authentication_fail_before_admission` | `grounding_gate_rejects_unknown_cross_kind_multi_paragraph_and_authoritative_drafts`; `invalid_classification_facts_fail_closed_before_the_language_layer`. |

## Localhost web journey

On 2026-07-23, the production `CoachWorkspace` ran against the local signed HTTP
binding and installed Stockfish/Maia providers with the canonical Lichess URL
`https://lichess.org/Synthet1Demo/black`.

- The picker displayed seven mixed moments in Game order:
  `10… b4` Positive, `11… Ba6` Improvement, `13… Bxb5` Improvement,
  `20… b3` Improvement, `33… d3` Positive, `36… Rb1` Improvement, and
  `39… Qeb3` Positive.
- Opening the first two moments showed their admitted, grounded comments rather
  than a frontend-authored replacement.
- The first hypothesis was corrected inline in Player wording. Navigating to
  the next moment and reopening the first preserved the correction and its
  authoritative Intent State.
- The Practice options rendered both eligible lessons, `Checkmate Patterns I`
  and `Piece Checkmates I`, grounded in the Improvement at ply 72. The web did
  not mislabel either candidate as selected before proposal validation.
- The journey exposed an unsupported generated-schema `uint64` format at the
  web decoder boundary. The runtime now accepts safe JSON integers for that
  Rust format, and the measured `gameImported` timing regression is automated.

## Coach Skill journey

The same URL completed through one persistent
`chenchess review-session --jsonl` process.

- The live picker returned seven mixed moments at plies 20, 22, 26, 40, 66, 72,
  and 78 with canonical Position refs and ordered grounded comments.
- A seven-paragraph Game Review draft passed
  `gameReviewValidation` without bypassing the CLI validator.
- Inspecting ply 26 returned the canonical Position Snapshot and evaluation;
  correcting its hypothesis preserved the same position identity while making
  the Player's original words authoritative.
- `Checkmate Patterns I` at ply 72 passed
  `practiceSelectionValidation` as an Improvement recommendation; the eligible
  candidates were `Checkmate Patterns I` and `Piece Checkmates I`.
- The CLI process was stopped and its ephemeral journey directory was moved to
  Trash after this evidence was recorded, leaving no journey artifacts in the
  repository or `/tmp`.

## Validation

The targeted Rust conformance tests, frontend App, decoder, and transport tests,
frontend typecheck/lint, Coach Skill contract tests, and CLI multi-moment
validator test pass. The mandatory structural review is clean. Connected Review
was explicitly deferred by the Player for this issue. The final local
release-proof result is recorded in the issue closing comment after this evidence
file is complete.
