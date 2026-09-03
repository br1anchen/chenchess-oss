# Commentary ladder (withheld)

The public snapshot does not carry this corpus.

Coach Engine's prose is measured against a ladder of human-annotated games:
recorded commentary aligned to positions, so a generated explanation can be
scored against what a strong human actually said about the same move. The
ladder is derived from third-party recordings. Republishing it here would
republish the source games and the people who played them, so the derived
records stay in the private development repository along with the raw material
they came from.

Nothing in the public product reads this directory. The behaviour the ladder
measures is covered publicly by the synthetic corpus in
`services/coach-engine/evaluation/corpus/`, which exercises the same fact
shapes, selector boundaries, and explanation grounding without carrying anyone
else's game.
