const fencedPgn = /```(?:pgn)?\s*([\s\S]*?)```/i

/**
 * The completed game inside a message, if there is one.
 *
 * Shared with the dashboard's structured import so both surfaces agree on what
 * counts as a pasted game: a fenced block or the first PGN-looking line, cut at
 * the last result token. Anything without a result is not a completed Game.
 */
export function extractCompletedPgn(message: string) {
  const fenced = fencedPgn.exec(message)?.[1]?.trim()
  if (fenced) return completedPgn(fenced)
  const tagStart = message.search(/^\s*\[[A-Za-z][^\]]*\]/m)
  const movetextStart = message.search(/^\s*1\.(?:\.\.)?\s+\S+/m)
  const start =
    tagStart >= 0 ? tagStart : movetextStart >= 0 ? movetextStart : -1
  if (start < 0) return null
  return completedPgn(message.slice(start).trim())
}

function completedPgn(candidate: string) {
  const results = [
    ...candidate.matchAll(/(?:^|\s)(1-0|0-1|1\/2-1\/2)(?=\s|$)/g),
  ]
  const completed = results.at(-1)
  if (!completed || completed.index === undefined) return null
  return candidate.slice(0, completed.index + completed[0].length).trim()
}
