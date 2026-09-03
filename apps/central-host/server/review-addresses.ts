/**
 * Reads the identifiers out of a Game Review resource URI.
 *
 * A resource URI is free text from whoever typed or replayed it, so an address
 * the Coach Engine could not have minted is answered from its own shape rather
 * than by spending a round trip to learn what the shape already says. Segment
 * lengths are deliberately not checked: the shared contract fixtures predate
 * the production digest widths, and an address with the right skeleton but the
 * wrong hex is a real miss the Engine should answer.
 */
import { ResourceNotFoundError } from "@modelcontextprotocol/server"

import {
  fromCriticalMomentId,
  fromGameImportId,
  type CriticalMomentId,
  type GameImportId,
} from "@chenchess/coach-engine-sdk"

/**
 * The Coach Engine's semantic-ID grammar: a 1-128 character opaque ASCII token
 * starting alphanumeric. An identifier outside it cannot be decoded on arrival,
 * so it is a malformed request rather than a missing review.
 */
const semanticId = /^[0-9A-Za-z][0-9A-Za-z._:-]{0,127}$/

export type UriVariable = string | string[] | undefined

export function gameImportAddress(
  uri: URL,
  variable: UriVariable,
  expectation: string,
): GameImportId {
  return fromGameImportId(address(uri, variable, "game-import", expectation))
}

/**
 * A Review Moment ID is `review-moment:{game digest}:{ply}`, so an address that
 * is not one names no moment in any review.
 */
export function reviewMomentAddress(
  uri: URL,
  variable: UriVariable,
  expectation: string,
): CriticalMomentId {
  const decoded = address(uri, variable, "review-moment", expectation)
  // The Coach Engine derives the last segment from a ply, so an address whose
  // ply is not a number names no moment in any Game and is answered here rather
  // than after a round trip. The digest segment is deliberately not checked —
  // see the note above on fixture widths.
  if (!/^[0-9]+$/.test(decoded.split(":")[2]!)) {
    throw new ResourceNotFoundError(uri.href, expectation)
  }
  return fromCriticalMomentId(decoded)
}

function address(
  uri: URL,
  variable: UriVariable,
  prefix: string,
  expectation: string,
) {
  const decoded = decodeVariable(variable)
  if (decoded && hasSegments(decoded, prefix)) return decoded
  throw new ResourceNotFoundError(uri.href, expectation)
}

/**
 * A template variable arrives percent-encoded or raw depending on who expanded
 * it, and every Coach Engine identifier contains colons either way.
 */
function decodeVariable(variable: UriVariable) {
  const raw = Array.isArray(variable) ? variable[0] : variable
  if (!raw) return undefined
  try {
    return decodeURIComponent(raw)
  } catch {
    // decodeURIComponent throws URIError on a malformed escape.
    return undefined
  }
}

function hasSegments(identifier: string, prefix: string) {
  if (!semanticId.test(identifier)) return false
  const segments = identifier.split(":")
  return (
    segments.length === 3 &&
    segments[0] === prefix &&
    segments.slice(1).every((segment) => segment.length > 0)
  )
}
