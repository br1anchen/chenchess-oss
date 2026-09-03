# Present Learning Tracks as Missing-Idea Paths

## Status

Accepted.

## Context

The Game Review Engine already selects evidence-backed Learning Tracks and
materializes exact Lichess resources, but the Review Session flattened those
tracks into anonymous `Learn` and `Drill` links. That presentation disconnected
the material from the idea demonstrated by the game and made each Critical
Moment feel like isolated move correction.

Lichess Practice and Puzzle Themes serve different instructional jobs.
Practice explains a concept through a fixed curriculum; themed puzzles train
recognition. Practice does not contain an exact module for every puzzle theme,
so a nearby lesson must not be substituted merely to fill the first stage.

## Decision

Present each nonempty moment-local Learning Track as one two-stage learning
path:

1. **Concept lesson:** the exact catalog-selected `Learn` resource. When the
   catalog has no exact Learn resource, retain the grounded Review Moment
   explanation and explicitly say that no exact Lichess Practice module was
   mapped.
2. **Pattern drilling:** the exact catalog-selected `Drill` resources.

Defer **Real-game application**. Repeating the supporting Review Moment adds
little after the Player has already seen its missing idea, engine move, and
grounded discussion. A future application stage must instead generate a novel
transfer position from the same idea category. That generation remains out of
scope until learning-material selection is stable.

Improvement support is labelled **Missing Idea**. Reinforcement support is
labelled **Idea Reinforced**. The currently supported concepts are grouped as:

| Learning concept      | Idea cluster               |
| --------------------- | -------------------------- |
| Fork                  | Piece Coordination         |
| Hanging piece         | Piece Coordination         |
| Passed-pawn promotion | Pawn Play                  |
| Exact opening mapping | Opening Tactical Awareness |

The Review Session timeline names an Improvement Opportunity by its Missing
Idea when a validated moment-local track exists. An improvement without a
semantic track remains an Improvement Opportunity; evaluation loss alone must
not manufacture a missing idea.

The Review Engine remains authoritative for track identity, support, and
resources. Presentation code may map the closed typed key vocabulary to
Player-facing names and clusters, but may not infer a new key, author a URL,
substitute a Practice module, expose rank, or present selection internals.

## Consequences

- Learning material now answers what the Player missed, how to understand it,
  and how to recognize it faster.
- Tracks without an exact Practice companion remain honest: they use the
  grounded explanation and exact themed drilling without inventing a third
  stage.
- Generated transfer positions require a separate future evidence contract;
  the supporting Review Moment must not masquerade as application practice.
- The broader cluster vocabulary can grow only when the Review Engine gains a
  typed detector and verified catalog mapping for a new idea.
- A future cross-game Idea Profile can aggregate the same typed track keys
  without parsing move-specific prose or accuracy labels.
