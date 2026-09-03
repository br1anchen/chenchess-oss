# Bound LLM Practice selection to deterministic evidence

## Status

Superseded by ADR 0035.

ADR 0009 keeps chess claims inside Rule Extraction, but semantic lesson choice benefits from the language layer used by both the web and Coach Skill paths. The Rule Extractor therefore emits closed Teaching Theme and Opening Principle facts, the Review Engine filters a bundled Practice Lesson Allowlist into a versioned Practice Selection Context, and the language layer compares every eligible lesson before choosing the single best ID or none. The Review Engine validates the supporting fact, owns the canonical URL, and omits an invalid selection, which intentionally broadens the LLM input beyond one serialized `RuleExtraction` without letting the model invent chess claims or links.
