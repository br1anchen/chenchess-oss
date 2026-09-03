# Game Review pipeline

ChenChess will generate a Game Review by running Engine Analysis and a Human Move Model in parallel, passing both outputs through a Rule Extractor, and using an LLM Explainer only to turn extracted facts into prose. This preserves the distinction between objective chess truth and human-likely decisions at the Player's Elo Profile, and prevents the LLM from becoming the source of chess claims. The MVP must include the full pipeline because its purpose is to demonstrate whether this approach can explain chess well to Amateur Players. The pipeline is designed by Elo Profile; kid-friendly wording is handled by Explanation Style in the LLM Explainer.

The MVP output is a Critical Moment Game Review: the pipeline selects teachable moments from the whole Game, and the Player can also nominate a Player-Selected Moment for on-demand review when their intuition disagrees with the pipeline.

## Local Pipeline Runtime refinement (2026-07-17, updated 2026-07-18)

Engine Analysis and the Human Move Model remain independent inputs to Rule Extraction, and Player-selected Position analysis continues to run them concurrently. Full-Game Local Coach Execution may sequence the provider phases when measurements on the certified runtime show that CPU contention makes overlap slower. The measured implementation runs eight-wide Stockfish work first, then four-wide Maia work. The phases do not overlap. This is an execution refinement, not a change to evidence ordering or ownership: both provider outputs must still be complete before Rule Extraction runs.
