# Coaching Board #491 — no-target is the board

Design Mind chrome lock. No-target `/app/board` is the Coaching Board. The
start position is visible. Import a game and Choose an opening live in the
right column (under the board on 375). Not a dialog. Not a first-step modal.

Required 375 and 1280 captures:

- **No target** — start board visible. Import a game / Choose an opening are
  tabs in the column, and the import form is already there. No empty paper.
  No modal.
- **Import filled** — Latest game full-width above URL/PGN when a signed-in
  profile game exists. Elo and Review side stay one line.
- **Game** — title, then graph → moments → plans.
- **Opening** — Najdorf this-ply. First paint 10/10, next `5…a6`.

Never say “lobby” or “Coaching Board” in UI copy. Header title is Coaching.
Game or opening stays on a loaded game or opening so the Player can change
target. It does not open a dialog as the no-target landing.

Shots come from `Pages/Coaching Board` stories `No Target`, `Import Latest`,
`Chosen Game`, and `Opening Line`.
