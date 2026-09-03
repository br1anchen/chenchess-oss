# ADR 0009: LLM Explainer fact boundary

## Status

Accepted.

## Decision

The production LLM Explainer contract accepts imported Game metadata, Explanation Style, and one serialized `RuleExtraction`. The Rule Extractor packet is the only source of chess claims. The system prompt explicitly forbids invented evaluations, lines, tactics, and best moves, and requires every move, score, probability, rank, and ply to be preserved.

The structured output contains a verdict and one explanation for every extracted Critical Moment keyed by its exact ply. Free-form lesson and training-plan fields are excluded; typed Learning Tracks and Learning Resources are selected by the Game Review Engine and projected through Review Moment Comment Facts. The Review Engine rejects empty required content, duplicate plies, missing extracted plies, and explanations for unknown plies even when a provider does not support JSON Schema enforcement.

Explanation Style is kept outside the fact packet. It changes wording instructions and fallback prose while the serialized Rule Extractor facts remain unchanged. OpenAI-compatible providers that reject `response_format` are retried without that parameter, but their JSON response is still parsed and validated locally.

The legacy bootstrap explainer remains callable only until the full Game Review Orchestrator supplies whole-Game Rule Extractor evidence in issue #8. That integration must switch the protected route to `explain_extracted_game` and remove the legacy fact path rather than preserving two production contracts.

ADR 0035 supersedes ADR 0015 and extends this boundary only with the active Review Moment's selected Learning Tracks. The language layer may explain those typed facts but cannot select, rank, replace, browse for, or author Learning Tracks, Learning Resources, or canonical URLs.
