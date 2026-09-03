// @vitest-environment jsdom

import {
  cleanup,
  render as renderView,
  screen,
  waitFor,
} from "@testing-library/react"
import type { ReactElement } from "react"
import { afterEach, expect, test, vi } from "vitest"
import { sharedGroundingSentences } from "@chenchess/shared-assets"
import { ChenTheme } from "@chenchess/ui/theme"

import {
  coachAppOnlyToolNames,
  coachWebBoardToolNames,
  coachWebLobbyToolNames,
  contractedCoachModelToolNames,
} from "../../server/board/tool-surface"
import {
  clearModelContextPolyfill,
  installModelContextPolyfill,
} from "@/coaching-board/modelContextPolyfill"
import { App } from "@/App"

import {
  TestFirebaseAuthProvider,
  type FirebaseIdentity,
} from "./FirebaseAuthProvider"
import { JoinIdentity } from "./JoinIdentity"
import { LoginIdentity } from "./LoginIdentity"
import { RouteRedirect } from "./RouteRedirect"
import {
  readSessionStatusDescription,
  sessionStatusOnJoin,
  sessionStatusOnLogin,
  sessionStatusResult,
} from "./sessionStatus"
import { useSessionStatusTool } from "./useSessionStatusTool"
import { coachingBoardDestination } from "./verifiedIdentityDestination"

afterEach(() => {
  cleanup()
  clearModelContextPolyfill()
  vi.unstubAllGlobals()
})

function renderAuth(ui: ReactElement, identity: FirebaseIdentity) {
  return renderView(ui, {
    wrapper: ({ children }) => (
      <TestFirebaseAuthProvider
        value={{
          fetchAccessToken: vi.fn().mockResolvedValue("firebase-token"),
          identity,
        }}
      >
        {children}
      </TestFirebaseAuthProvider>
    ),
  })
}

function signedOut(): FirebaseIdentity {
  return { kind: "signedOut" }
}

function unverified(): FirebaseIdentity {
  return {
    email: "player@example.test",
    emailVerified: false,
    kind: "signedIn",
    playerId: "firebase-player",
  }
}

function verified(): FirebaseIdentity {
  return {
    email: "player@example.test",
    emailVerified: true,
    kind: "signedIn",
    playerId: "firebase-player",
  }
}

test("session-status descriptions assemble shared grounding, not a fork", () => {
  for (const sentence of sharedGroundingSentences) {
    expect(readSessionStatusDescription).toContain(sentence)
  }
  expect(readSessionStatusDescription).toContain("signed out")
  expect(readSessionStatusDescription).toContain("email unverified")
  expect(readSessionStatusDescription).toContain("no Beta Access")
})

test("reports the locked stage and the href that resolves it", () => {
  expect(sessionStatusOnLogin(signedOut(), coachingBoardDestination)).toEqual({
    href: "/login/",
    stage: "signedOut",
  })
  expect(sessionStatusOnLogin(unverified(), coachingBoardDestination)).toEqual({
    href: "/login/",
    stage: "emailUnverified",
  })
  expect(sessionStatusOnLogin(verified(), coachingBoardDestination)).toBeNull()
  expect(
    sessionStatusOnJoin({ kind: "required" }, coachingBoardDestination),
  ).toEqual({
    href: "/join/",
    stage: "noBetaAccess",
  })
  expect(
    sessionStatusOnJoin({ kind: "checking" }, coachingBoardDestination),
  ).toBeNull()
  expect(
    sessionStatusOnJoin(
      { kind: "granted", playerId: "firebase-player" },
      coachingBoardDestination,
    ),
  ).toBeNull()
  expect(
    sessionStatusResult({
      href: "/login/",
      stage: "signedOut",
    }),
  ).toMatchObject({
    constraints: { kind: "constraints" },
    href: "/login/",
    kind: "sessionStatus",
    stage: "signedOut",
  })
})

test("the Sign-In Page registers a read-only session-status tool when signed out", async () => {
  const tools = installModelContextPolyfill()
  renderAuth(
    <LoginIdentity
      initialInvitationCode={null}
      navigate={vi.fn()}
      verifiedDestination={coachingBoardDestination}
    />,
    signedOut(),
  )
  expect([...tools.keys()]).toEqual(["read_session_status"])
  const tool = tools.get("read_session_status")
  expect(tool?.annotations).toEqual({
    idempotentHint: true,
    readOnlyHint: true,
  })
  expect(tool?.description).toBe(readSessionStatusDescription)
  const listed = await tool?.execute({})
  expect(listed?.structuredContent).toMatchObject({
    href: "/login/",
    kind: "sessionStatus",
    stage: "signedOut",
  })
  expect(coachWebBoardToolNames.some((name) => tools.has(name))).toBe(false)
  expect(coachWebLobbyToolNames.some((name) => tools.has(name))).toBe(false)
})

function StatusHarness({
  status,
}: {
  status: Parameters<typeof useSessionStatusTool>[0]
}) {
  useSessionStatusTool(status)
  return null
}

test("a mounted locked-stage tool is retracted when the stage resolves or the surface unmounts", async () => {
  const tools = installModelContextPolyfill()
  const locked = {
    href: "/join/",
    stage: "noBetaAccess",
  } as const
  const view = renderView(<StatusHarness status={locked} />)
  expect([...tools.keys()]).toEqual(["read_session_status"])

  view.rerender(<StatusHarness status={null} />)
  expect([...tools.keys()]).toEqual([])

  view.rerender(<StatusHarness status={locked} />)
  expect([...tools.keys()]).toEqual(["read_session_status"])
  view.unmount()
  expect([...tools.keys()]).toEqual([])
})

test("the Sign-In Page registers session-status when email is unverified", async () => {
  const tools = installModelContextPolyfill()
  renderAuth(
    <LoginIdentity
      initialInvitationCode={null}
      navigate={vi.fn()}
      verifiedDestination={coachingBoardDestination}
    />,
    unverified(),
  )
  expect(
    screen.getByRole("heading", { name: "Verify your email" }),
  ).toBeTruthy()
  expect([...tools.keys()]).toEqual(["read_session_status"])
  const listed = await tools.get("read_session_status")?.execute({})
  expect(listed?.structuredContent).toMatchObject({
    href: "/login/",
    stage: "emailUnverified",
  })
})

test("the Sign-In Page registers nothing when it is about to open Beta Admission", async () => {
  const tools = installModelContextPolyfill()
  vi.stubGlobal(
    "fetch",
    vi
      .fn<typeof fetch>()
      .mockResolvedValue(new Response(null, { status: 403 })),
  )
  const navigate = vi.fn()
  renderAuth(
    <LoginIdentity
      initialInvitationCode={null}
      navigate={navigate}
      verifiedDestination={coachingBoardDestination}
    />,
    verified(),
  )
  await waitFor(() => expect(navigate).toHaveBeenCalledWith("/join/"))
  expect([...tools.keys()]).toEqual([])
})

test("the Beta Admission Page registers session-status when Beta Access is required", async () => {
  const tools = installModelContextPolyfill()
  vi.stubGlobal(
    "fetch",
    vi
      .fn<typeof fetch>()
      .mockResolvedValue(new Response(null, { status: 403 })),
  )
  renderAuth(
    <JoinIdentity
      initialInvitationCode={null}
      navigate={vi.fn()}
      verifiedDestination={coachingBoardDestination}
    />,
    verified(),
  )
  expect(
    await screen.findByRole("heading", { name: "Ask for an invite" }),
  ).toBeTruthy()
  expect([...tools.keys()]).toEqual(["read_session_status"])
  const tool = tools.get("read_session_status")
  expect(tool?.annotations?.readOnlyHint).toBe(true)
  const listed = await tool?.execute({})
  expect(listed?.structuredContent).toMatchObject({
    href: "/join/",
    kind: "sessionStatus",
    stage: "noBetaAccess",
  })
  expect(coachWebBoardToolNames.some((name) => tools.has(name))).toBe(false)
})

test("the Beta Admission Page registers nothing when it is about to open sign-in", () => {
  const tools = installModelContextPolyfill()
  const navigate = vi.fn()
  renderAuth(
    <JoinIdentity
      initialInvitationCode={null}
      navigate={navigate}
      verifiedDestination={coachingBoardDestination}
    />,
    signedOut(),
  )
  expect(navigate).toHaveBeenCalledWith("/login/")
  expect([...tools.keys()]).toEqual([])
})

test("a redirect shell that is about to navigate away registers nothing", () => {
  const tools = installModelContextPolyfill()
  renderView(
    <RouteRedirect
      description="Sign in to continue."
      href="/login/"
      navigate={vi.fn()}
      title="Opening sign-in"
    />,
  )
  expect([...tools.keys()]).toEqual([])
})

test("the unauthorized /app/ gate registers nothing on the way to sign-in", async () => {
  const tools = installModelContextPolyfill()
  const navigate = vi.fn()
  renderView(
    <ChenTheme>
      <TestFirebaseAuthProvider value={{ identity: signedOut() }}>
        <App navigate={navigate} pathname="/app/" />
      </TestFirebaseAuthProvider>
    </ChenTheme>,
  )
  await waitFor(() => expect(navigate).toHaveBeenCalledWith("/login/"))
  expect([...tools.keys()]).toEqual([])
})

test("session-status is web-only and does not join the MCP name lists", () => {
  expect(contractedCoachModelToolNames).not.toContain("read_session_status")
  expect(coachAppOnlyToolNames).not.toContain("read_session_status")
  expect(coachWebBoardToolNames).not.toContain("read_session_status")
  expect(coachWebLobbyToolNames).not.toContain("read_session_status")
})
