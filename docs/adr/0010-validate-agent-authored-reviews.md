# Validate agent-authored reviews before presentation

The Coach Skill will have the active coding agent write the same structured narrative used by the web LLM Explainer, then call a deterministic Review Validator before presenting the Game Review. The validator rejects empty required content and duplicate, missing, or unknown Critical Moment plies. This extra round trip preserves the existing structural fact boundary; skill instructions and manual evaluation remain responsible for semantic claims in the prose.
