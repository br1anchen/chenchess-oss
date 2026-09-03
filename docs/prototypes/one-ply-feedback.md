# One-ply feedback UI decision

Decision date: 2026-07-13

## Question

What Coach UI should let a Player report one displayed Critical Moment or nominate one omitted reviewable ply without a whole-review rating or automatic upload?

## Chosen layout

Use the inline feedback layout from prototype A with the explicit move and ply selector from prototype C.

The left side starts with two target groups:

- **Displayed Critical Moments** shows every selected moment as a large choice with move number, SAN, category, and a short explanation preview.
- **Nominate an omitted move** shows legal reviewable plies that the selector omitted. Choosing one creates selection feedback with the fixed reason `should-select`.

The chosen target stays visible below the selector. The adjacent feedback form updates in place, so the Player does not leave the Game Review or enter a modal flow.

## Feedback form

For a displayed Critical Moment, require one fixed reason code:

- `should-not-select`
- `severity-exaggerated`
- `unsupported-claim`
- `wrong-interpretation`
- `plan-misunderstood`
- `variation-wrong`
- `not-useful`

For an omitted ply, show `should-select` as a fixed reason rather than a dropdown.

Both paths accept an optional Player plan. They target exactly one positive ply. There is no star rating, thumbs control, free-form category, or whole-review score.

## Disclosure and GitHub handoff

Keep the redacted JSON disclosure in the same form, collapsed by default but available before any action. It includes the target, reason, optional plan, sanitized PGN, compact selector context, selected facts, generated review, and version provenance. It excludes Player identity, identifying PGN headers, and raw provider dumps.

The actions are:

1. **Copy report** copies the complete Markdown or canonical JSON report locally.
2. **Open GitHub issue** opens a short prefilled URL with title and labels only.
3. The Player inspects and pastes the copied report manually.

The client performs no background submission and sends no report body through the URL.

## Responsive behavior

On narrow screens, stack the target selector above the feedback form. Keep the two target groups distinct and preserve the disclosure step before the manual GitHub handoff.

## Prototype verdict

Brian selected prototype A and replaced its dropdown and per-card report entry points with prototype C's explicit move and ply choices. The denser feedback desk and the three-step wizard are rejected as the primary layout.
