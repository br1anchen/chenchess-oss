# Coaching Board #484 — v1 UI lock shots

Required 375 and 1280 captures:

- No-target `/app/board` — board shell, dialog open, two equal-width
  exits (Import a game / Choose an opening) on one row. No target
  title. Desktop header: BrandLockup + Coaching + Game or opening.
  Mobile header: BrandLockup + Game or opening; Coaching sits above
  the board stack and fills the width.
- Import form in the dialog — URL or PGN, Elo and Review side on one
  line with labels on the same baseline.
- Opening find in the dialog — ECO / name / line. Same one-row exits.
- Chosen-game board — game title at the top of the right column, then
  graph → moments → plans. ConversationPanel not mounted.
- Opening-line board — opening title at the top of the right column,
  then next-move branches from this ply, then Ideas. Najdorf first
  paint: 10/10, next 5…a6.

Never say “lobby” in UI copy. Header title is Coaching, not Coaching
Board. Shots come from `Pages/Coaching Board` stories.
