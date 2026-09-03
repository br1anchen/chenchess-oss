import { expect, test, vi } from "vitest"

import { captureInvitationCode } from "./invitationFragment"

test("captures one invitation fragment and removes it without a navigation", () => {
  const replaceState = vi.fn()
  const state = { journey: "join" }

  expect(
    captureInvitationCode(
      {
        hash: "#invite=ABCDEF0123456789ABCDEF0123456789",
        pathname: "/join/",
        search: "?return_to=backoffice",
      },
      { replaceState, state },
    ),
  ).toBe("ABCDEF0123456789ABCDEF0123456789")
  expect(replaceState).toHaveBeenCalledWith(
    state,
    "",
    "/join/?return_to=backoffice",
  )
})

test("scrubs ambiguous or malformed fragments without accepting a code", () => {
  for (const hash of ["#invite=first&invite=second", "#unrelated=secret"]) {
    const replaceState = vi.fn()
    expect(
      captureInvitationCode(
        { hash, pathname: "/join/", search: "" },
        { replaceState, state: null },
      ),
    ).toBeNull()
    expect(replaceState).toHaveBeenCalledWith(null, "", "/join/")
  }
})

test("does not rewrite a join URL that has no fragment", () => {
  const replaceState = vi.fn()
  expect(
    captureInvitationCode(
      { hash: "", pathname: "/join/", search: "" },
      { replaceState, state: null },
    ),
  ).toBeNull()
  expect(replaceState).not.toHaveBeenCalled()
})
