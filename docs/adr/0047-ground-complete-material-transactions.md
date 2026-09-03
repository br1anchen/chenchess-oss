# Ground Complete Material Transactions

## Status

Accepted.

## Context

The model-visible Decision Explanation recited at most six plies from a selected
candidate. That keeps prose readable, but it hid later material events still
present in the durable legal line. A coach could therefore describe an apparent
sacrifice—such as collecting a knight and later losing a rook—without reaching
the bishop recovery that completed the same Engine line.

Candidate Generation Proof also retained an exact Position Goal, but the
grounded projection omitted it. The model either lacked the proof-backed goal
or risked inventing an Engine intention from the variation.

## Decision

Each grounded Explanation Path exposes the Position Goal from its existing
Candidate Generation Proof when one exists. The goal remains candidate-finding
evidence, not Engine intent, and absence remains absence.

The same path derives an ordered material transaction from every capture and
promotion in the selected candidate's complete `line_steps`. Each event carries
its line ply, UCI and SAN, mover, typed material change, and a signed conventional
value delta from the root side's perspective. The transaction carries the
versioned value policy and net delta. Capture, promotion, and
capture-with-promotion are distinct variants, so no nullable event combination
can express an impossible non-event.

The existing six-ply spoken variation limit remains unchanged. Transaction
projection happens at read time from already durable legal steps; it neither
retains pruned outcomes nor changes Decision Explanation identity or generation.

## Consequences

Coaching can state the immediate collection, intervening loss, and final
recovery in their true order without reciting a long line. A path with no
generation proof or no material events carries no placeholder, and the host is
instructed to infer neither. SAN remains presentation derived from legal UCI and
position snapshots rather than a source of chess facts.
