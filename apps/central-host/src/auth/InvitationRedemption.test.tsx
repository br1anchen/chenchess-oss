// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, expect, test, vi } from "vitest"

import { InvitationRedemption } from "./InvitationRedemption"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

test("prefills a securely captured link code and redeems with a fresh token", async () => {
  const fetchAccessToken = vi.fn().mockResolvedValue("fresh-firebase-token")
  const fetchMock = vi.fn().mockResolvedValue(
    new Response('{"outcome":"granted"}', {
      headers: { "Content-Type": "application/json" },
      status: 200,
    }),
  )
  vi.stubGlobal("fetch", fetchMock)
  const user = userEvent.setup()
  const onRedeemed = vi.fn()

  render(
    <InvitationRedemption
      fetchAccessToken={fetchAccessToken}
      initialCode="ABCDEF0123456789ABCDEF0123456789"
      onRedeemed={onRedeemed}
    />,
  )
  expect(screen.getByText("Invitation link captured securely.")).toBeTruthy()
  expect(screen.getByLabelText("Invitation code")).toHaveProperty(
    "value",
    "ABCDEF0123456789ABCDEF0123456789",
  )

  await user.click(screen.getByRole("button", { name: "Redeem invitation" }))

  expect(fetchAccessToken).toHaveBeenCalledWith({ forceRefreshToken: true })
  expect(fetchMock).toHaveBeenCalledWith(
    "/api/v1/beta-access/invitations/redeem",
    expect.objectContaining({
      body: JSON.stringify({
        code: "ABCDEF0123456789ABCDEF0123456789",
      }),
      headers: {
        Authorization: "Bearer fresh-firebase-token",
        "Content-Type": "application/json",
      },
      method: "POST",
    }),
  )
  expect(
    await screen.findByText("Access granted to this account."),
  ).toBeTruthy()
  expect(screen.getByLabelText("Invitation code")).toHaveProperty("value", "")
  expect(onRedeemed).toHaveBeenCalledOnce()
})

test("accepts a manual code and presents safe non-consuming failure outcomes", async () => {
  const fetchAccessToken = vi.fn().mockResolvedValue("fresh-firebase-token")
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(
      new Response('{"outcome":"wrongAccount"}', { status: 200 }),
    )
    .mockResolvedValueOnce(
      new Response('{"outcome":"verificationRequired"}', { status: 200 }),
    )
    .mockResolvedValueOnce(
      new Response('{"outcome":"tryLater"}', { status: 200 }),
    )
  vi.stubGlobal("fetch", fetchMock)
  const user = userEvent.setup()

  render(
    <InvitationRedemption
      fetchAccessToken={fetchAccessToken}
      initialCode={null}
      onRedeemed={vi.fn()}
    />,
  )
  const input = screen.getByLabelText("Invitation code")
  await user.type(input, "0123456789abcdef0123456789abcdef")

  await user.click(screen.getByRole("button", { name: "Redeem invitation" }))
  expect(await screen.findByText(/different email address/i)).toBeTruthy()
  await user.click(screen.getByRole("button", { name: "Redeem invitation" }))
  expect(
    await screen.findByText(/Verify the invited email address/i),
  ).toBeTruthy()
  await user.click(screen.getByRole("button", { name: "Redeem invitation" }))
  expect(await screen.findByText(/too many attempts/i)).toBeTruthy()
  expect(input).toHaveProperty("value", "0123456789abcdef0123456789abcdef")
})

test("fails closed on unavailable tokens and malformed API responses", async () => {
  const fetchAccessToken = vi
    .fn()
    .mockResolvedValueOnce(null)
    .mockResolvedValueOnce("fresh-firebase-token")
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue(new Response('{"outcome":"surprise"}')),
  )
  const user = userEvent.setup()

  render(
    <InvitationRedemption
      fetchAccessToken={fetchAccessToken}
      initialCode="0123456789abcdef0123456789abcdef"
      onRedeemed={vi.fn()}
    />,
  )
  await user.click(screen.getByRole("button", { name: "Redeem invitation" }))
  expect(await screen.findByText(/temporarily unavailable/i)).toBeTruthy()
  await user.click(screen.getByRole("button", { name: "Redeem invitation" }))
  expect(await screen.findByText(/temporarily unavailable/i)).toBeTruthy()
})
