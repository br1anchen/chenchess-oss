/**
 * The generated contract fixtures, narrowed to the completion a test needs.
 *
 * The fixture stream is the one input three separate projections agree on, so
 * reaching into it is worth doing once rather than in every test file.
 */
import { events, type OperationCompletion } from "@chenchess/coach-engine-sdk"

export function completionFixture<Kind extends OperationCompletion["kind"]>(
  kind: Kind,
): Extract<OperationCompletion, { kind: Kind }> {
  const completion = events.find(
    ({ event }) => event.kind === "completed" && event.result?.kind === kind,
  )?.event
  if (!completion || completion.kind !== "completed") {
    throw new Error(`Generated fixtures have no ${kind} completion`)
  }
  return structuredClone(
    // SAFETY: Kind is the same discriminant events.find already matched on result.kind.
    completion.result as Extract<OperationCompletion, { kind: Kind }>,
  )
}
