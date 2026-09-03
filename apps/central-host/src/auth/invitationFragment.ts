type JoinLocation = Pick<Location, "hash" | "pathname" | "search">
type JoinHistory = Pick<History, "replaceState" | "state">

export function captureInvitationCode(
  location: JoinLocation,
  history: JoinHistory,
): string | null {
  if (location.hash.length === 0) return null

  const invitationCodes = new URLSearchParams(location.hash.slice(1)).getAll(
    "invite",
  )
  history.replaceState(
    history.state,
    "",
    `${location.pathname}${location.search}`,
  )
  if (invitationCodes.length !== 1) return null
  const code = invitationCodes[0] ?? ""
  return /^[0-9a-f]{32}$/i.test(code) ? code : null
}
