# Domain Docs

How engineering skills consume this repo’s domain documentation.

## Before exploring, read these

- **`CONTEXT.md`** at the repo root. Start at **Do not restore**, then the
  term you need. Do not treat superseded ADRs as the live model.
- **`docs/adr/`** — read ADRs that touch the area you are about to change.
  Status `superseded` means history. ADR 0026 and ADR 0042 override earlier
  intent-lifecycle and Review Session Checkpoint clauses.

If either location does not exist, proceed silently.

## File structure

Single-context layout (no per-package glossary):

```
/
├── CONTEXT.md
├── CODING_STANDARDS.md
├── docs/adr/
├── apps/                     # deployables; do not import services/
├── services/                 # deployables; do not import apps/
├── packages/                 # shared templates, fixtures, data, logic
└── skills/chenchess-coach/   # Player-facing Coach Skill, not the glossary
```

## Use the glossary’s vocabulary

When output names a domain concept — issue title, refactor, hypothesis, or
test name — use the term defined in `CONTEXT.md`. Do not drift to synonyms the
glossary explicitly avoids, and do not restore a retired term.

If a needed concept is absent, note the gap for `grill-with-types` or local
`/domain-modeling`. Do not invent a product name.

## Flag ADR conflicts

If output contradicts an accepted ADR, surface the conflict instead of
silently overriding the decision.

## Completed ADR cuts are not test subjects

The live rule lives in `CODING_STANDARDS.md`. If a reviewer asks for a
"do not regress to X" test after X is gone from the type system, refuse
and point there.
