// @vitest-environment jsdom

import {
  cleanup,
  fireEvent,
  render as renderView,
  screen,
  waitFor,
  within,
} from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactElement } from "react"
import {
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  test,
  vi,
} from "vitest"

import {
  decodeReviewSessionCommandEnvelope,
  fromAlternativeMoveId,
  fromGameImportId,
} from "@chenchess/coach-engine-sdk"
import { TestFirebaseAuthProvider } from "@/auth/FirebaseAuthProvider"
import { ChenTheme } from "@chenchess/ui/theme"
import { App, CoachWorkspace } from "./App"
import type { GameImportId } from "@chenchess/coach-engine-sdk"
import { forkLearningMaterial } from "./review-session/learningMaterialTestFixtures"
import { reviewWithLazyMoment } from "./review-session/lazyMomentTestFixtures"
import {
  hostTurnRefusalText,
  hostTurnStepLabels,
} from "./review-session/thread-state"
import { INTERACTIVE_COACHING_UNAVAILABLE } from "./review-session/useReviewSessionCommands"
import {
  fixtureCore,
  fixtureGameReview,
  loadReviewSessionFixtures,
  parseEnabledPreference,
  preparedCoreAtPly,
  FIXTURE_GAME_IMPORT_ID,
  reviewSessionResponder,
  type EngineOpenSlot,
  type FailOpenSlot,
  type ReviewMomentOpenHold,
  type ReviewSessionFixtureOptions,
} from "./review-session/reviewSessionStreamFixtures"

type MockIdentity =
  | { kind: "signedOut" }
  | {
      kind: "signedIn"
      email: string
      emailVerified: boolean
      playerId: string
    }

type FirebaseAuthDouble = {
  fetchAccessToken: ReturnType<typeof vi.fn>
  identity: MockIdentity
  providerLink: { kind: "none" }
  reauthenticate: ReturnType<typeof vi.fn>
  signOut: ReturnType<typeof vi.fn>
}

const firebaseAuth = vi.hoisted(
  (): FirebaseAuthDouble => ({
    fetchAccessToken: vi.fn(),
    identity: { kind: "signedOut" },
    providerLink: { kind: "none" },
    reauthenticate: vi.fn(),
    signOut: vi.fn(),
  }),
)

function render(ui: ReactElement) {
  return renderView(ui, {
    wrapper: ({ children }) => (
      <ChenTheme>
        <TestFirebaseAuthProvider value={firebaseAuth}>
          {children}
        </TestFirebaseAuthProvider>
      </ChenTheme>
    ),
  })
}

beforeAll(async () => {
  await loadReviewSessionFixtures()
})

beforeEach(() => {
  firebaseAuth.identity = { kind: "signedOut" }
  firebaseAuth.fetchAccessToken.mockReset()
})

afterEach(() => {
  cleanup()
  firebaseAuth.identity = { kind: "signedOut" }
  firebaseAuth.fetchAccessToken.mockReset()
  localStorage.clear()
  vi.unstubAllGlobals()
})

test("anonymous Players can visit the no-target Coaching Board", async () => {
  const navigate = vi.fn()
  render(<App navigate={navigate} pathname="/app/board" />)
  expect(await screen.findByRole("main", { name: "Coaching" })).toBeTruthy()
  expect(
    document.querySelector(".chen-watercolor-session-subtitle")?.textContent,
  ).toBe("Coaching")
  expect(screen.queryByText("Coaching Board")).toBeNull()
  expect(screen.queryByRole("button", { name: "Game or opening" })).toBeNull()
  expect(screen.queryByRole("dialog")).toBeNull()
  expect(screen.getByRole("button", { name: "Import a game" })).toBeTruthy()
  expect(screen.getByRole("button", { name: "Choose an opening" })).toBeTruthy()
  expect(screen.getByLabelText("Game URL or PGN")).toBeTruthy()
  expect(screen.queryByRole("heading", { name: "Choose a game" })).toBeNull()
  expect(screen.queryByText("Lobby")).toBeNull()
  expect(navigate).not.toHaveBeenCalled()
})

test("anonymous Players can visit a Coaching Board game address", async () => {
  const navigate = vi.fn()
  render(
    <App
      navigate={navigate}
      pathname="/app/board/games/game-import%3Atest%3Alinked"
    />,
  )
  expect(await screen.findByRole("main", { name: "Coaching" })).toBeTruthy()
  expect(screen.queryByText("Coaching Board")).toBeNull()
  expect(navigate).not.toHaveBeenCalled()
  expect(
    screen.queryByRole("article", { name: "Interactive coaching" }),
  ).toBeNull()
})

test("unauthenticated Players enter sign-in instead of seeing game data", async () => {
  const navigate = vi.fn()
  render(<App navigate={navigate} />)
  await waitFor(() => expect(navigate).toHaveBeenCalledWith("/login/"))
  expect(
    screen.queryByRole("heading", { name: "Start a Review Session" }),
  ).toBeNull()
})

test("preserves a validated Review Moment link through sign-in", async () => {
  const navigate = vi.fn()
  render(
    <App
      navigate={navigate}
      pathname="/app/game-reviews/game-import%3Atest%3Alinked/moments/review-moment%3Atest%3Aone"
    />,
  )

  await waitFor(() =>
    expect(navigate).toHaveBeenCalledWith(
      "/login/?return_to=app&game_review=game-import%3Atest%3Alinked&review_moment=review-moment%3Atest%3Aone",
    ),
  )
})

test("sends the deleted Review Session link nowhere in particular", async () => {
  const navigate = vi.fn()
  render(
    <App
      navigate={navigate}
      pathname="/app/review-sessions/game-import%3Atest%3Alinked"
    />,
  )

  await waitFor(() => expect(navigate).toHaveBeenCalledWith("/login/"))
})

test("preserves a validated durable Game Review link through sign-in", async () => {
  const navigate = vi.fn()
  render(
    <App
      navigate={navigate}
      pathname="/app/game-reviews/game-import%3Atest%3Alinked"
    />,
  )

  await waitFor(() =>
    expect(navigate).toHaveBeenCalledWith(
      "/login/?return_to=app&game_review=game-import%3Atest%3Alinked",
    ),
  )
})

test("unverified Players return to sign-in for email verification", async () => {
  firebaseAuth.identity = {
    kind: "signedIn",
    email: "player@example.com",
    emailVerified: false,
    playerId: "firebase-player-test",
  }

  const navigate = vi.fn()
  render(<App navigate={navigate} />)
  await waitFor(() => expect(navigate).toHaveBeenCalledWith("/login/"))
  expect(
    screen.queryByRole("heading", { name: "Start a Review Session" }),
  ).toBeNull()
})

test("verified Players without current Beta Access stay outside the product", async () => {
  firebaseAuth.identity = verifiedIdentity()
  firebaseAuth.fetchAccessToken.mockResolvedValue("firebase-token")
  vi.stubGlobal(
    "fetch",
    vi
      .fn<typeof fetch>()
      .mockResolvedValue(new Response(null, { status: 403 })),
  )

  const navigate = vi.fn()
  render(<App navigate={navigate} />)
  await waitFor(() => expect(navigate).toHaveBeenCalledWith("/join/"))
  expect(
    screen.queryByRole("heading", { name: "Start a Review Session" }),
  ).toBeNull()
})

test("preserves a validated continuation link through Beta admission", async () => {
  firebaseAuth.identity = verifiedIdentity()
  firebaseAuth.fetchAccessToken.mockResolvedValue("firebase-token")
  vi.stubGlobal(
    "fetch",
    vi
      .fn<typeof fetch>()
      .mockResolvedValue(new Response(null, { status: 403 })),
  )

  const navigate = vi.fn()
  render(
    <App
      navigate={navigate}
      pathname="/app/game-reviews/game-import%3Atest%3Alinked/moments/review-moment%3Atest%3Aone/sequences/engineBest"
    />,
  )

  await waitFor(() =>
    expect(navigate).toHaveBeenCalledWith(
      "/join/?return_to=app&game_review=game-import%3Atest%3Alinked&review_moment=review-moment%3Atest%3Aone&sequence=engineBest",
    ),
  )
})

test("checks the current grant again when Firebase refreshes the Player session", async () => {
  let admitted = true
  firebaseAuth.identity = verifiedIdentity()
  firebaseAuth.fetchAccessToken.mockResolvedValue("firebase-token")
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockImplementation(async (input) => {
      if (!String(input).endsWith("/api/v1/beta-access/authorization")) {
        return new Response(null, { status: 503 })
      }
      return admitted
        ? Response.json({ playerId: "firebase-player-test" })
        : new Response(null, { status: 403 })
    }),
  )

  const navigate = vi.fn()
  const view = render(<App navigate={navigate} />)
  // An admitted Player at the retired app address is sent to the Coaching Board.
  await waitFor(() => expect(navigate).toHaveBeenCalledWith("/app/board"))

  admitted = false
  firebaseAuth.identity = verifiedIdentity()
  view.rerender(<App navigate={navigate} />)
  await waitFor(() => expect(navigate).toHaveBeenCalledWith("/join/"))
})

describe("authenticated Review Session", () => {
  test("replays the verified canonical game into the production board and conversation", async () => {
    const fetchMock = reviewSessionFetch()
    vi.stubGlobal("fetch", fetchMock)
    renderWorkspace()

    expect(
      await screen.findByRole("main", { name: "Game review" }),
    ).toBeTruthy()
    expect(screen.getByText(/Black · A00 · Saragossa Opening/)).toBeTruthy()
    expect(screen.getByRole("button", { name: /45… Qd1#/ })).toBeTruthy()
    expect(screen.getAllByRole("gridcell")).toHaveLength(64)
    expect(
      await screen.findByRole("img", {
        name: "Measured real-game evaluation graph",
      }),
    ).toBeTruthy()
    expect(await screen.findByText(/My best guess is/)).toBeTruthy()
    expect(screen.queryByText(/Good: e4 advanced/)).toBeNull()
    expect(screen.queryByText(/confidence/i)).toBeNull()
    expect(
      screen.queryByRole("link", {
        name: "Open this Review Moment on its own page",
      }),
    ).toBeNull()
    expect(screen.queryByText(/Sending feedback/)).toBeNull()
    expect(screen.getByText("Helpful?")).toBeTruthy()

    const commands = await postedCommands(fetchMock)
    expect(commands.map((command) => command.command.kind)).toEqual([
      "startReviewSession",
    ])
    expect(commands[0]?.command).toMatchObject({
      kind: "startReviewSession",
    })
    expect(fetchMock.mock.calls[0]?.[1]?.headers).toEqual(
      expect.objectContaining({ Authorization: "Bearer review-jwt" }),
    )
  })

  test("opens an addressed Game as a Review Session without a frozen gate", async () => {
    const gameImportId = fromGameImportId("game-import:test:web")
    const fetchMock = reviewSessionFetch()
    vi.stubGlobal("fetch", fetchMock)

    renderWorkspace({ initialGameImportId: gameImportId })

    expect(
      await screen.findByRole("main", { name: "Game review" }),
    ).toBeTruthy()
    expect(
      screen.getByRole("button", { name: "Account settings" }),
    ).toBeTruthy()
    expect(screen.getByRole("button", { name: "Log out" })).toBeTruthy()
    expect(screen.getByRole("main").dataset.hasConversation).toBe("true")
    expect(
      screen.queryByRole("article", { name: "Interactive coaching" }),
    ).toBeNull()
    expect(
      screen.queryByRole("button", { name: "Start interactive review" }),
    ).toBeNull()
    expect(screen.queryByRole("heading", { name: "Game Review" })).toBeNull()
    expect(
      (await postedCommands(fetchMock)).map(({ command }) => command.kind),
    ).toEqual(["startReviewSession"])
  })

  test("offers an account switch and a way back when an authenticated Game link is unavailable", async () => {
    const signOut = vi.fn(async () => undefined)
    const fetchMock = reviewSessionFetch({
      rejectSessionStart: "unknownGameImport",
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()

    renderWorkspace({
      initialGameImportId: fromGameImportId("game-import:test:web:unavailable"),
      signOut,
    })

    expect(
      await screen.findByRole("heading", { name: "Link unavailable" }),
    ).toBeTruthy()
    expect(
      screen.getByText("player@example.com").closest("p")?.textContent,
    ).toBe(
      "This link is not available on the account signed in as player@example.com.",
    )
    // A Player already on the right account has no account to switch to, so
    // the card is a dead end without this.
    expect(
      screen.getByRole("link", { name: "Back to the Coaching Board" }),
    ).toHaveProperty("href", "http://localhost:3000/app/board")
    await user.click(
      screen.getByRole("button", { name: "Log out and switch account" }),
    )
    expect(signOut).toHaveBeenCalledOnce()
    expect(
      (await postedCommands(fetchMock)).map(({ command }) => command.kind),
    ).toEqual(["startReviewSession"])
  })

  test("keeps a frozen Game Review with no Automatic Review Moments resumable", async () => {
    const review = structuredClone(fixtureGameReview())
    review.criticalMoments = []
    const fetchMock = reviewSessionFetch({
      preparedReviewMoments: [],
      review,
    })
    vi.stubGlobal("fetch", fetchMock)

    renderWorkspace({
      initialGameImportId: fromGameImportId("game-import:test:web"),
    })

    expect(
      await screen.findByRole("heading", {
        name: "No key moments found",
      }),
    ).toBeTruthy()
    expect(
      screen.queryByRole("button", { name: "Start interactive review" }),
    ).toBeNull()
    expect(
      (await postedCommands(fetchMock)).map(({ command }) => command.kind),
    ).toEqual(["startReviewSession"])
  })

  test("clears the known session on sign-out and terminal expiry", async () => {
    const signOut = vi.fn(async () => undefined)
    const fetchMock = reviewSessionFetch()
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace({ signOut })
    await screen.findByRole("main", { name: "Game review" })

    await user.click(screen.getByRole("button", { name: "Log out" }))
    expect(signOut).toHaveBeenCalledOnce()

    cleanup()
    vi.stubGlobal("fetch", reviewSessionFetch({ rejectSessionStart: true }))
    renderWorkspace()
    expect(await screen.findByText(/This review is out of date/)).toBeTruthy()

    cleanup()
    vi.stubGlobal(
      "fetch",
      reviewSessionFetch({ rejectSessionStart: "unknownGameImport" }),
    )
    renderWorkspace()
    // An addressed review whose Game Import is unknown is a link this account
    // cannot open, not a session failure.
    expect(
      await screen.findByRole("heading", { name: "Link unavailable" }),
    ).toBeTruthy()
  })

  test("updates the same improvement preference from account settings", async () => {
    const fetchMock = reviewSessionFetch({ retentionAvailable: true })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()

    await screen.findByRole("main", { name: "Game review" })
    await user.click(screen.getByRole("button", { name: "Account settings" }))
    await user.click(checkboxTarget(accountSettingsQualityCapturePreference()))

    expect(preferenceUpdates(fetchMock)).toEqual([{ enabled: false }])
    expect(
      screen.getByText(/stops new copies immediately.*deleted or withdrawn/),
    ).toBeTruthy()
  })

  test("withdraws the Quality Capture Preference from Account Settings before first-run disclosure is acknowledged", async () => {
    const fetchMock = reviewSessionFetch({ retentionAvailable: true })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()

    await screen.findByRole("main", { name: "Game review" })
    await user.click(screen.getByRole("button", { name: "Account settings" }))
    expect(
      screen.getAllByRole("checkbox", { name: "Help improve coaching" }),
    ).toHaveLength(1)
    const preference = accountSettingsQualityCapturePreference()
    expect(preference).toHaveProperty("checked", true)
    await user.click(checkboxTarget(preference))

    expect(preferenceUpdates(fetchMock)).toEqual([{ enabled: false }])
    expect(accountSettingsQualityCapturePreference()).toHaveProperty(
      "checked",
      false,
    )
  })

  test("keeps Account Settings when the Quality Capture Preference cannot be read", async () => {
    vi.stubGlobal("fetch", reviewSessionFetch({ rejectPreference: true }))
    const user = userEvent.setup()
    renderWorkspace()

    await screen.findByRole("main", { name: "Game review" })
    await user.click(screen.getByRole("button", { name: "Account settings" }))
    expect(
      screen.queryByRole("checkbox", { name: "Help improve coaching" }),
    ).toBeNull()
    expect(
      await screen.findByRole("heading", { name: "Delete account" }),
    ).toBeTruthy()
    await waitFor(() => {
      expect(
        accountSettingsRoot().querySelector("[role='alert']")?.textContent,
      ).toContain(
        "Coach Engine artifact retention preference failed with HTTP 500",
      )
    })
  })

  test("reauthenticates and confirms both environments before account deletion", async () => {
    const reviewFetch = reviewSessionFetch({ retentionAvailable: true })
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockImplementation((input, init) =>
        String(input) === "/api/v1/account/deletion"
          ? Promise.resolve(new Response(null, { status: 204 }))
          : reviewFetch(input, init),
      )
    vi.stubGlobal("fetch", fetchMock)
    const reauthenticate = vi.fn(async () => undefined)
    const signOut = vi.fn(async () => undefined)
    const user = userEvent.setup()
    renderWorkspace({ reauthenticate, signOut })

    await screen.findByRole("main", { name: "Game review" })
    await user.click(screen.getByRole("button", { name: "Account settings" }))
    await user.type(
      screen.getByLabelText("Current password"),
      "current-password",
    )
    await user.click(
      checkboxTarget(
        screen.getByRole("checkbox", {
          name: /deletes my account in both staging and production/i,
        }),
      ),
    )
    await user.click(
      screen.getByRole("button", { name: "Permanently delete account" }),
    )

    await vi.waitFor(() => expect(signOut).toHaveBeenCalledOnce())
    expect(reauthenticate).toHaveBeenCalledWith("current-password")
    const deletion = fetchMock.mock.calls.find(
      ([input]) => String(input) === "/api/v1/account/deletion",
    )
    expect(deletion?.[1]).toMatchObject({
      body: JSON.stringify({
        confirmation: "DELETE MY CHEN CHESS ACCOUNT IN STAGING AND PRODUCTION",
      }),
      headers: {
        Authorization: "Bearer review-jwt",
        "Content-Type": "application/json",
      },
      method: "POST",
    })
  })

  test("supports keyboard board navigation in the narrow-screen journey", async () => {
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      value: 480,
    })
    vi.stubGlobal("fetch", reviewSessionFetch())
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })

    const e2 = screen.getByRole("gridcell", { name: /e2 white pawn/ })
    e2.focus()
    await user.keyboard("{ArrowRight}")
    expect(document.activeElement).toBe(
      screen.getByRole("gridcell", { name: /d2 white pawn/ }),
    )
    expect(screen.getByLabelText("Full game move list")).toBeTruthy()
    expect(screen.getByLabelText("Coaching conversation")).toBeTruthy()
  })

  test("sends only legal destination moves and ignores a conflicting branch result", async () => {
    const fetchMock = reviewSessionFetch()
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    await user.click(screen.getByRole("gridcell", { name: /e2 white pawn/ }))
    await user.click(
      screen.getByRole("gridcell", { name: /e4 empty, legal destination/ }),
    )
    await waitForCommandCount(fetchMock, 3)

    expect((await postedCommands(fetchMock))[2]?.command).toMatchObject({
      kind: "exploreAlternativeMove",
      parent: { kind: "root" },
      moveInput: { kind: "uci", uci: "e2e4" },
    })
    expect(
      await screen.findByText(/newer result replaced this one/),
    ).toBeTruthy()
    expect(screen.queryByLabelText("Alternative branches")).toBeNull()
  })

  test("renders objective Alternative Move evaluation before optional continuation and branch coaching", async () => {
    const fetchMock = reviewSessionFetch({
      alternativeScenario: "success",
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    await user.click(screen.getByRole("gridcell", { name: /e2 white pawn/ }))
    await user.click(
      screen.getByRole("gridcell", { name: /e4 empty, legal destination/ }),
    )
    await waitForCommandCount(fetchMock, 4)
    expect(
      (await postedCommands(fetchMock)).map(({ command }) => command.kind),
    ).toEqual([
      "startReviewSession",
      "openReviewMoment",
      "exploreAlternativeMove",
      "inspectPosition",
    ])

    expect(
      await screen.findByRole("button", {
        name: /e4 · \+0.22/,
      }),
    ).toBeTruthy()
    expect(screen.queryByRole("button", { name: /e2e4/ })).toBeNull()
    expect(
      screen.getByRole("button", {
        name: "Best move: e5",
      }),
    ).toBeTruthy()
    expect(screen.queryByText(/Coach target/)).toBeNull()
    expect(screen.queryByText(/Stockfish evaluated/)).toBeNull()
    expect(screen.queryByText(/e2e4/)).toBeNull()
    expect(
      (await postedCommands(fetchMock)).filter(
        ({ command }) => command.kind === "exploreAlternativeMove",
      ),
    ).toHaveLength(1)

    await user.type(
      await followUpComposer(user),
      "How resilient is this branch?",
    )
    await user.click(screen.getByRole("button", { name: "Send" }))

    await waitForCommandCount(fetchMock, 5)
    await user.type(
      await followUpComposer(user),
      "Focus on the strongest counterplay instead.",
    )
    await user.click(screen.getByRole("button", { name: "Send" }))

    await waitFor(() => {
      expect(screen.getAllByText("The knight was hanging.")).toHaveLength(2)
    })
    const hostTurns = (await postedCommands(fetchMock)).filter(
      ({ command }) => command.kind === "startHostTurn",
    )
    expect(hostTurns).toHaveLength(2)
    expect(hostTurns[1]?.command).toMatchObject({
      priorTurns: [
        {
          message: "How resilient is this branch?",
          answer: "The knight was hanging.",
        },
      ],
    })
    expect(
      (await postedCommands(fetchMock)).filter(
        ({ command }) => command.kind === "cancelOperation",
      ),
    ).toHaveLength(0)
  })

  test("keeps Player plan discussion conversational and removes intent lifecycle controls", async () => {
    const fetchMock = reviewSessionFetch({})
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    expect(
      screen.queryByRole("button", { name: "Yes, that was my plan" }),
    ).toBeNull()
    expect(screen.queryByRole("button", { name: "No" })).toBeNull()
    expect(screen.queryByRole("button", { name: "Skip" })).toBeNull()
    expect(
      screen.queryByRole("button", { name: "Explain another intent" }),
    ).toBeNull()
    expect(
      screen.queryByRole("button", { name: "Answer clarification" }),
    ).toBeNull()

    const plan = "I wanted to occupy the center before developing."
    await user.type(await followUpComposer(user), plan)
    await user.click(screen.getByRole("button", { name: "Send" }))

    expect(screen.getByText(plan)).toBeTruthy()
    expect(await screen.findByText("The knight was hanging.")).toBeTruthy()
    expect(
      (await postedCommands(fetchMock)).map(({ command }) => command.kind),
    ).toEqual(["startReviewSession", "openReviewMoment", "startHostTurn"])
  })

  test("a question with no Alternative Move active reaches Coach Engine", async () => {
    const fetchMock = reviewSessionFetch({})
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    await user.type(
      await followUpComposer(user),
      "Why was this move a mistake?",
    )
    await user.click(screen.getByRole("button", { name: "Send" }))

    expect(screen.getByText("Why was this move a mistake?")).toBeTruthy()
    expect(await screen.findByText("The knight was hanging.")).toBeTruthy()
    const hostTurn = (await postedCommands(fetchMock)).find(
      ({ command }) => command.kind === "startHostTurn",
    )
    expect(hostTurn?.command).toMatchObject({
      kind: "startHostTurn",
      priorTurns: [],
    })
    expect(screen.queryByText(INTERACTIVE_COACHING_UNAVAILABLE)).toBeNull()
  })

  test("renders HostTurn unavailability as a thread message", async () => {
    const fetchMock = reviewSessionFetch({ hostTurn: { kind: "unavailable" } })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    await user.type(
      await followUpComposer(user),
      "Why was this move a mistake?",
    )
    await user.click(screen.getByRole("button", { name: "Send" }))

    expect(
      await screen.findByText(INTERACTIVE_COACHING_UNAVAILABLE),
    ).toBeTruthy()
    expect(screen.getByText("Why was this move a mistake?")).toBeTruthy()
  })

  test("renders HostTurn refusal as a thread message", async () => {
    const fetchMock = reviewSessionFetch({
      hostTurn: { kind: "refused", reason: "notAboutThisReview" },
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    await user.type(
      await followUpComposer(user),
      "What is the Sicilian Defense?",
    )
    await user.click(screen.getByRole("button", { name: "Send" }))

    expect(
      await screen.findByText(hostTurnRefusalText.notAboutThisReview),
    ).toBeTruthy()
  })

  test("shows each HostTurn step label while the turn is in flight", async () => {
    const hold: ReviewMomentOpenHold = {}
    const fetchMock = reviewSessionFetch({
      hostTurn: { kind: "held", hold },
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    await user.type(await followUpComposer(user), "Which moments matter?")
    await user.click(screen.getByRole("button", { name: "Send" }))

    expect(await screen.findByText(hostTurnStepLabels.writing)).toBeTruthy()
    hold.release?.()
    expect(await screen.findByText("The knight was hanging.")).toBeTruthy()
    for (const label of Object.values(hostTurnStepLabels)) {
      expect(label).not.toMatch(
        /read_moment|list_moments|evaluate_line|learning_material|capability/i,
      )
    }
  })

  test("disables the composer while a HostTurn is in flight", async () => {
    const hold: ReviewMomentOpenHold = {}
    const fetchMock = reviewSessionFetch({
      hostTurn: { kind: "held", hold },
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    await user.type(await followUpComposer(user), "First question.")
    await user.click(screen.getByRole("button", { name: "Send" }))

    expect(await screen.findByText(hostTurnStepLabels.writing)).toBeTruthy()
    expect(await followUpComposer(user)).toHaveProperty("disabled", true)
    expect(screen.getByRole("button", { name: "Send" })).toHaveProperty(
      "disabled",
      true,
    )
    expect(
      (await postedCommands(fetchMock)).filter(
        ({ command }) => command.kind === "startHostTurn",
      ),
    ).toHaveLength(1)

    hold.release?.()
    expect(await screen.findByText("The knight was hanging.")).toBeTruthy()
    expect(
      (await postedCommands(fetchMock)).filter(
        ({ command }) => command.kind === "startHostTurn",
      ),
    ).toHaveLength(1)
  })

  test("cancel does not render language-layer outage copy", async () => {
    const hold: ReviewMomentOpenHold = {}
    const fetchMock = reviewSessionFetch({
      hostTurn: { kind: "held", hold },
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    await user.type(await followUpComposer(user), "Please wait.")
    await user.click(screen.getByRole("button", { name: "Send" }))
    expect(await screen.findByText(hostTurnStepLabels.writing)).toBeTruthy()

    await user.click(screen.getByRole("button", { name: "Cancel" }))
    expect(
      await screen.findByText("Cancelled. Nothing was added."),
    ).toBeTruthy()
    expect(screen.queryByText(INTERACTIVE_COACHING_UNAVAILABLE)).toBeNull()
  })

  test("focusMoment on an unseen ply walks the board without nominating", async () => {
    const initial = fixtureCore()
    const fetchMock = reviewSessionFetch({
      preparedReviewMoments: [initial],
      hostTurn: { kind: "answer", focusMoment: 3 },
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    await user.type(await followUpComposer(user), "Look at the later move.")
    await user.click(screen.getByRole("button", { name: "Send" }))

    expect(await screen.findByText("The knight was hanging.")).toBeTruthy()
    expect(screen.getByText("Discuss this position?")).toBeTruthy()
    const nominated = (await postedCommands(fetchMock)).filter(
      ({ command }) =>
        command.kind === "openReviewMoment" &&
        command.selection.kind === "playerSelectedMoment",
    )
    expect(nominated).toHaveLength(0)
  })

  test("focusMoment switches the open Review Moment in place", async () => {
    const initial = fixtureCore()
    const later = preparedCoreAtPly(3)
    const review = reviewWithLazyMoment(
      fixtureGameReview(),
      initial.importedGame,
    )
    const fetchMock = reviewSessionFetch({
      preparedReviewMoments: [initial, later],
      review,
      hostTurn: { kind: "answer", focusMoment: later.reviewMoment.ply },
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    await user.type(await followUpComposer(user), "Show me the later moment.")
    await user.click(screen.getByRole("button", { name: "Send" }))

    expect(await screen.findByRole("heading", { name: "2. d4" })).toBeTruthy()
    expect(await screen.findByText("Show me the later moment.")).toBeTruthy()
    expect(screen.getByText("The knight was hanging.")).toBeTruthy()
    const opened = (await postedCommands(fetchMock)).filter(
      ({ command }) =>
        command.kind === "openReviewMoment" &&
        command.selection.kind === "pipelineCriticalMoment" &&
        command.selection.criticalMomentId === later.reviewMoment.momentId,
    )
    expect(opened.length).toBeGreaterThan(0)

    await waitFor(async () => {
      expect(await followUpComposer(user)).toHaveProperty("disabled", false)
    })
    await user.type(await followUpComposer(user), "What about this one?")
    await user.click(screen.getByRole("button", { name: "Send" }))
    await waitFor(async () => {
      expect(
        (await postedCommands(fetchMock)).filter(
          ({ command }) => command.kind === "startHostTurn",
        ),
      ).toHaveLength(2)
    })
    const hostTurns = (await postedCommands(fetchMock)).filter(
      ({ command }) => command.kind === "startHostTurn",
    )
    expect(hostTurns[1]?.command).toMatchObject({
      priorTurns: [
        {
          message: "Show me the later moment.",
          answer: "The knight was hanging.",
        },
      ],
    })
  })

  test("refuses Send while exploration is in flight and still navigates focusMoment after it settles", async () => {
    const initial = fixtureCore()
    const later = preparedCoreAtPly(3)
    const review = reviewWithLazyMoment(
      fixtureGameReview(),
      initial.importedGame,
    )
    const hold: ReviewMomentOpenHold = {}
    const fetchMock = reviewSessionFetch({
      alternativeScenario: "success",
      holdAlternative: hold,
      preparedReviewMoments: [initial, later],
      review,
      hostTurn: [
        { kind: "answer", focusMoment: later.reviewMoment.ply },
        { kind: "answer" },
      ],
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    await user.type(await followUpComposer(user), "Show me the later moment.")
    await user.click(screen.getByRole("gridcell", { name: /e2 white pawn/ }))
    await user.click(
      screen.getByRole("gridcell", { name: /e4 empty, legal destination/ }),
    )

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Send" })).toHaveProperty(
        "disabled",
        true,
      )
    })
    fireEvent.click(screen.getByRole("button", { name: "Send" }))
    expect(
      (await postedCommands(fetchMock)).filter(
        ({ command }) => command.kind === "startHostTurn",
      ),
    ).toHaveLength(0)

    hold.release?.()
    expect(screen.queryByText(/Stockfish evaluated/)).toBeNull()
    await waitFor(async () => {
      expect(await followUpComposer(user)).toHaveProperty("disabled", false)
    })
    expect(await followUpComposer(user)).toHaveProperty(
      "value",
      "Show me the later moment.",
    )
    await user.click(screen.getByRole("button", { name: "Send" }))

    expect(await screen.findByRole("heading", { name: "2. d4" })).toBeTruthy()
    expect(await screen.findByText("Show me the later moment.")).toBeTruthy()
    expect(screen.getByText("The knight was hanging.")).toBeTruthy()
    const opened = (await postedCommands(fetchMock)).filter(
      ({ command }) =>
        command.kind === "openReviewMoment" &&
        command.selection.kind === "pipelineCriticalMoment" &&
        command.selection.criticalMomentId === later.reviewMoment.momentId,
    )
    expect(opened.length).toBeGreaterThan(0)

    await waitFor(async () => {
      expect(await followUpComposer(user)).toHaveProperty("disabled", false)
    })
    await user.type(await followUpComposer(user), "What about this one?")
    await user.click(screen.getByRole("button", { name: "Send" }))
    await waitFor(async () => {
      expect(
        (await postedCommands(fetchMock)).filter(
          ({ command }) => command.kind === "startHostTurn",
        ),
      ).toHaveLength(2)
    })
    const hostTurns = (await postedCommands(fetchMock)).filter(
      ({ command }) => command.kind === "startHostTurn",
    )
    expect(hostTurns[1]?.command).toMatchObject({
      priorTurns: [
        {
          message: "Show me the later moment.",
          answer: "The knight was hanging.",
        },
      ],
    })
  })

  test("showLine renders the named engine line on the board", async () => {
    const fetchMock = reviewSessionFetch({
      hostTurn: { kind: "answer", showLine: { kind: "engineBest" } },
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    await user.type(await followUpComposer(user), "Show the engine line.")
    await user.click(screen.getByRole("button", { name: "Send" }))

    expect(await screen.findByText("The knight was hanging.")).toBeTruthy()
    expect(await screen.findByText("Engine line")).toBeTruthy()
    const inspected = (await postedCommands(fetchMock)).filter(
      ({ command }) =>
        command.kind === "inspectPosition" &&
        command.target.kind === "reviewedMove",
    )
    expect(inspected.length).toBeGreaterThan(0)
  })

  test("a later answer without showLine clears the board line badge", async () => {
    const fetchMock = reviewSessionFetch({
      hostTurn: [
        { kind: "answer", showLine: { kind: "engineBest" } },
        { kind: "answer" },
      ],
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    await user.type(await followUpComposer(user), "Show the engine line.")
    await user.click(screen.getByRole("button", { name: "Send" }))
    expect(await screen.findByText("Engine line")).toBeTruthy()

    await waitFor(async () => {
      expect(await followUpComposer(user)).toHaveProperty("disabled", false)
    })
    await user.type(await followUpComposer(user), "Back to the moment.")
    await user.click(screen.getByRole("button", { name: "Send" }))
    await waitFor(() => {
      expect(screen.queryByText("Engine line")).toBeNull()
    })
    expect(screen.getByRole("heading", { name: "1. c3" })).toBeTruthy()
    expect(screen.queryByText("Engine line")).toBeNull()
  })

  test("a rejected HostTurn replies beside the Player message", async () => {
    const fetchMock = reviewSessionFetch({
      hostTurn: { kind: "rejected" },
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    await user.type(
      await followUpComposer(user),
      "Why was this move a mistake?",
    )
    await user.click(screen.getByRole("button", { name: "Send" }))

    expect(await screen.findByText("Why was this move a mistake?")).toBeTruthy()
    expect(
      await screen.findByText("Correct the highlighted input and try again."),
    ).toBeTruthy()
    expect(screen.queryByText(INTERACTIVE_COACHING_UNAVAILABLE)).toBeNull()
  })

  test("showLine alternativeMove inspects and renders the named line", async () => {
    const alternativeMoveId = fromAlternativeMoveId(
      "alternative-move:web:host-line",
    )
    const fetchMock = reviewSessionFetch({
      hostTurn: {
        kind: "answer",
        showLine: { kind: "alternativeMove", alternativeMoveId },
      },
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    await user.type(await followUpComposer(user), "Show that alternative.")
    await user.click(screen.getByRole("button", { name: "Send" }))

    expect(await screen.findByText("The knight was hanging.")).toBeTruthy()
    expect(await screen.findByText("Alternative branch")).toBeTruthy()
    expect(screen.queryByRole("button", { name: /Best move:/ })).toBeNull()
    const inspected = (await postedCommands(fetchMock)).filter(
      ({ command }) =>
        command.kind === "inspectPosition" &&
        command.target.kind === "alternativeMove" &&
        command.target.alternativeMoveId === alternativeMoveId,
    )
    expect(inspected.length).toBeGreaterThan(0)
  })

  test("one answer with focusMoment and showLine inspects the open moment then navigates", async () => {
    const initial = fixtureCore()
    const later = preparedCoreAtPly(3)
    const review = reviewWithLazyMoment(
      fixtureGameReview(),
      initial.importedGame,
    )
    const fetchMock = reviewSessionFetch({
      preparedReviewMoments: [initial, later],
      review,
      hostTurn: {
        kind: "answer",
        focusMoment: later.reviewMoment.ply,
        showLine: { kind: "engineBest" },
      },
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    await user.type(
      await followUpComposer(user),
      "Look at the later moment and that line.",
    )
    await user.click(screen.getByRole("button", { name: "Send" }))

    expect(await screen.findByRole("heading", { name: "2. d4" })).toBeTruthy()
    expect(await screen.findByText("The knight was hanging.")).toBeTruthy()
    expect(screen.queryByText("Engine line")).toBeNull()
    expect(
      screen.queryByText("Correct the highlighted input and try again."),
    ).toBeNull()
    expect(screen.queryByText(/unknownTarget/)).toBeNull()
    expect(screen.getByRole("heading", { name: "2. d4" })).toBeTruthy()
    expect(screen.queryByText("Engine line")).toBeNull()

    const commands = await postedCommands(fetchMock)
    const inspected = commands.filter(
      ({ command }) =>
        command.kind === "inspectPosition" &&
        command.target.kind === "reviewedMove",
    )
    expect(inspected).toHaveLength(1)
    expect(inspected[0]?.command).toMatchObject({
      kind: "inspectPosition",
      reviewMomentId: initial.reviewMoment.momentId,
      target: { kind: "reviewedMove" },
    })
    expect(inspected[0]?.command).not.toMatchObject({
      reviewMomentId: later.reviewMoment.momentId,
    })
    const opened = commands.filter(
      ({ command }) =>
        command.kind === "openReviewMoment" &&
        command.selection.kind === "pipelineCriticalMoment" &&
        command.selection.criticalMomentId === later.reviewMoment.momentId,
    )
    expect(opened.length).toBeGreaterThan(0)
    const inspectIndex = commands.findIndex(
      ({ command }) => command.kind === "inspectPosition",
    )
    const focusIndex = commands.findIndex(
      ({ command }) =>
        command.kind === "openReviewMoment" &&
        command.selection.kind === "pipelineCriticalMoment" &&
        command.selection.criticalMomentId === later.reviewMoment.momentId,
    )
    expect(inspectIndex).toBeGreaterThan(-1)
    expect(focusIndex).toBeGreaterThan(inspectIndex)
  })

  test("revisiting an already-opened Review Moment re-syncs Coach Engine before the next HostTurn", async () => {
    const initial = fixtureCore()
    const later = preparedCoreAtPly(3)
    const review = reviewWithLazyMoment(
      fixtureGameReview(),
      initial.importedGame,
    )
    const fetchMock = reviewSessionFetch({
      preparedReviewMoments: [initial, later],
      review,
      hostTurn: [
        { kind: "answer", focusMoment: initial.reviewMoment.ply },
        { kind: "answer", showLine: { kind: "engineBest" } },
      ],
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    const picker = screen.getByLabelText("Full game move list")
    await user.click(within(picker).getByRole("button", { name: /2\. d4/ }))
    expect(await screen.findByRole("heading", { name: "2. d4" })).toBeTruthy()
    await waitFor(async () => {
      expect(await followUpComposer(user)).toHaveProperty("disabled", false)
    })

    await user.type(
      await followUpComposer(user),
      "Take me back to the first moment.",
    )
    await user.click(screen.getByRole("button", { name: "Send" }))
    expect(await screen.findByRole("heading", { name: "1. c3" })).toBeTruthy()
    expect(await screen.findByText("The knight was hanging.")).toBeTruthy()
    await waitFor(async () => {
      expect(await followUpComposer(user)).toHaveProperty("disabled", false)
    })

    await user.type(await followUpComposer(user), "Show the engine line.")
    await user.click(screen.getByRole("button", { name: "Send" }))
    expect(await screen.findByText("Engine line")).toBeTruthy()
    expect(screen.getByRole("heading", { name: "1. c3" })).toBeTruthy()

    const commands = await postedCommands(fetchMock)
    const laterOpenIndex = commands.findIndex(
      ({ command }) =>
        command.kind === "openReviewMoment" &&
        command.selection.kind === "pipelineCriticalMoment" &&
        command.selection.criticalMomentId === later.reviewMoment.momentId,
    )
    const revisitOpenIndex = commands.findIndex(
      ({ command }, index) =>
        index > laterOpenIndex &&
        command.kind === "openReviewMoment" &&
        command.selection.kind === "pipelineCriticalMoment" &&
        command.selection.criticalMomentId === initial.reviewMoment.momentId,
    )
    const hostTurnIndexes = commands.flatMap(({ command }, index) =>
      command.kind === "startHostTurn" ? [index] : [],
    )
    const inspected = commands.filter(
      ({ command }) =>
        command.kind === "inspectPosition" &&
        command.target.kind === "reviewedMove",
    )
    expect(laterOpenIndex).toBeGreaterThan(-1)
    expect(revisitOpenIndex).toBeGreaterThan(laterOpenIndex)
    expect(hostTurnIndexes).toHaveLength(2)
    expect(hostTurnIndexes[1]).toBeGreaterThan(revisitOpenIndex)
    expect(inspected).toHaveLength(1)
    expect(inspected[0]?.command).toMatchObject({
      kind: "inspectPosition",
      reviewMomentId: initial.reviewMoment.momentId,
      target: { kind: "reviewedMove" },
    })
    expect(inspected[0]?.command).not.toMatchObject({
      reviewMomentId: later.reviewMoment.momentId,
    })
    const inspectIndex = commands.findIndex(
      ({ command }) => command.kind === "inspectPosition",
    )
    expect(inspectIndex).toBeGreaterThan(revisitOpenIndex)
  })

  test("selecting another Critical Moment opens it for authored commentary", async () => {
    const initial = fixtureCore()
    const later = preparedCoreAtPly(3)
    const review = reviewWithLazyMoment(
      fixtureGameReview(),
      initial.importedGame,
    )
    const engineOpen: EngineOpenSlot = { ply: null }
    const fetchMock = reviewSessionFetch({
      preparedReviewMoments: [initial, later],
      review,
      engineOpen,
      hostTurn: { kind: "answer", showLine: { kind: "engineBest" } },
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    expect(await screen.findByText(/My best guess is/)).toBeTruthy()
    expect(document.querySelector("[data-comment-wait=bounded]")).toBeNull()
    expect(
      (await postedCommands(fetchMock)).map(({ command }) => command.kind),
    ).toEqual(["startReviewSession"])

    const picker = screen.getByLabelText("Full game move list")
    const laterMoment = within(picker).getByRole("button", { name: /2\. d4/ })
    expect(laterMoment).toHaveProperty("disabled", false)
    await user.click(laterMoment)
    expect(await screen.findByRole("heading", { name: "2. d4" })).toBeTruthy()
    // Selection opens the moment in the background so the Engine authors its
    // comment; navigation stays unlocked meanwhile.
    await waitFor(() => {
      expect(engineOpen.ply).toBe(later.reviewMoment.ply)
    })
    expect(
      (await postedCommands(fetchMock)).filter(
        ({ command }) => command.kind === "openReviewMoment",
      ),
    ).toHaveLength(1)

    await user.type(await followUpComposer(user), "Show the engine line.")
    await user.click(screen.getByRole("button", { name: "Send" }))
    expect(await screen.findByText("Engine line")).toBeTruthy()
    expect(screen.getByRole("heading", { name: "2. d4" })).toBeTruthy()
    expect(engineOpen.ply).toBe(later.reviewMoment.ply)
    const opened = (await postedCommands(fetchMock)).filter(
      ({ command }) =>
        command.kind === "openReviewMoment" &&
        command.selection.kind === "pipelineCriticalMoment" &&
        command.selection.criticalMomentId === later.reviewMoment.momentId,
    )
    expect(opened).toHaveLength(1)
    const inspected = (await postedCommands(fetchMock)).filter(
      ({ command }) =>
        command.kind === "inspectPosition" &&
        command.target.kind === "reviewedMove",
    )
    expect(inspected).toHaveLength(1)
    expect(inspected[0]?.command).toMatchObject({
      kind: "inspectPosition",
      reviewMomentId: later.reviewMoment.momentId,
      target: { kind: "reviewedMove" },
    })
  })

  test("a failed newest open keeps the previous workspace authoritative for HostTurn", async () => {
    const initial = fixtureCore()
    const later = preparedCoreAtPly(3)
    const review = reviewWithLazyMoment(
      fixtureGameReview(),
      initial.importedGame,
    )
    const engineOpen: EngineOpenSlot = { ply: null }
    const fetchMock = reviewSessionFetch({
      preparedReviewMoments: [initial, later],
      review,
      failOpenPly: { current: later.reviewMoment.ply },
      engineOpen,
      hostTurn: { kind: "answer", showLine: { kind: "engineBest" } },
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)
    expect(engineOpen.ply).toBeNull()

    await user.type(await followUpComposer(user), "Show the engine line.")
    await user.click(screen.getByRole("button", { name: "Send" }))
    expect(await screen.findByText("Engine line")).toBeTruthy()
    expect(engineOpen.ply).toBe(initial.reviewMoment.ply)

    const picker = screen.getByLabelText("Full game move list")
    await user.click(within(picker).getByRole("button", { name: /2\. d4/ }))
    expect(await screen.findByRole("heading", { name: "2. d4" })).toBeTruthy()
    expect(engineOpen.ply).toBe(initial.reviewMoment.ply)

    await waitFor(async () => {
      expect(await followUpComposer(user)).toHaveProperty("disabled", false)
    })
    await user.type(await followUpComposer(user), "Ask about this later move.")
    await user.click(screen.getByRole("button", { name: "Send" }))
    expect(
      await screen.findByText(/Correct the highlighted input and try again/),
    ).toBeTruthy()
    expect(engineOpen.ply).toBe(initial.reviewMoment.ply)
    expect(screen.getByRole("heading", { name: "2. d4" })).toBeTruthy()

    const commands = await postedCommands(fetchMock)
    const laterOpens = commands.filter(
      ({ command }) =>
        command.kind === "openReviewMoment" &&
        command.selection.kind === "pipelineCriticalMoment" &&
        command.selection.criticalMomentId === later.reviewMoment.momentId,
    )
    const hostTurns = commands.filter(
      ({ command }) => command.kind === "startHostTurn",
    )
    // One background open on selection, one retry on the send.
    expect(laterOpens).toHaveLength(2)
    expect(hostTurns).toHaveLength(1)
    expect(hostTurns[0]?.command).toMatchObject({
      kind: "startHostTurn",
    })
  })

  test("a failed reload open on a resident session cannot dispatch HostTurn on the stale ply", async () => {
    const initial = fixtureCore()
    const later = preparedCoreAtPly(3)
    const review = reviewWithLazyMoment(
      fixtureGameReview(),
      initial.importedGame,
    )
    const failOpenPly: FailOpenSlot = { current: null }
    const engineOpen: EngineOpenSlot = { ply: null }
    const fetchMock = reviewSessionFetch({
      preparedReviewMoments: [initial, later],
      review,
      failOpenPly,
      engineOpen,
      hostTurn: { kind: "answer", showLine: { kind: "engineBest" } },
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)
    expect(engineOpen.ply).toBeNull()

    await user.type(await followUpComposer(user), "Show the engine line.")
    await user.click(screen.getByRole("button", { name: "Send" }))
    expect(await screen.findByText("Engine line")).toBeTruthy()
    expect(engineOpen.ply).toBe(initial.reviewMoment.ply)

    const picker = screen.getByLabelText("Full game move list")
    await user.click(within(picker).getByRole("button", { name: /2\. d4/ }))
    expect(await screen.findByRole("heading", { name: "2. d4" })).toBeTruthy()
    await waitFor(async () => {
      expect(await followUpComposer(user)).toHaveProperty("disabled", false)
    })
    await user.type(await followUpComposer(user), "Ask about this later move.")
    await user.click(screen.getByRole("button", { name: "Send" }))
    await waitFor(() => {
      expect(engineOpen.ply).toBe(later.reviewMoment.ply)
    })

    const commandsBeforeReload = await postedCommands(fetchMock)
    failOpenPly.current = initial.reviewMoment.ply
    cleanup()
    const reloaded = userEvent.setup()
    renderWorkspace()
    expect(
      await screen.findByRole("main", { name: "Game review" }),
    ).toBeTruthy()
    expect(await screen.findByRole("heading", { name: "1. c3" })).toBeTruthy()
    expect(engineOpen.ply).toBe(later.reviewMoment.ply)
    expect(screen.queryByRole("heading", { name: "2. d4" })).toBeNull()

    expect(await followUpComposer(reloaded)).toHaveProperty("disabled", false)

    const commandsAfterReload = await postedCommands(fetchMock)
    const reloadCommands = commandsAfterReload.slice(
      commandsBeforeReload.length,
    )
    expect(
      reloadCommands.some(
        ({ command }) => command.kind === "startReviewSession",
      ),
    ).toBe(true)
    expect(
      reloadCommands.filter(
        ({ command }) => command.kind === "openReviewMoment",
      ),
    ).toHaveLength(0)
    expect(
      reloadCommands.filter(({ command }) => command.kind === "startHostTurn"),
    ).toHaveLength(0)
    expect(engineOpen.ply).toBe(later.reviewMoment.ply)
    expect(screen.getByRole("heading", { name: "1. c3" })).toBeTruthy()

    await reloaded.type(
      await followUpComposer(reloaded),
      "Show the engine line.",
    )
    await reloaded.click(screen.getByRole("button", { name: "Send" }))
    expect(
      await screen.findByText(/Correct the highlighted input and try again/),
    ).toBeTruthy()
    const commandsAfterFailedSend = await postedCommands(fetchMock)
    expect(
      commandsAfterFailedSend
        .slice(commandsAfterReload.length)
        .filter(({ command }) => command.kind === "startHostTurn"),
    ).toHaveLength(0)
    expect(engineOpen.ply).toBe(later.reviewMoment.ply)
    expect(screen.getByRole("heading", { name: "1. c3" })).toBeTruthy()

    failOpenPly.current = null
    await waitFor(async () => {
      expect(await followUpComposer(reloaded)).toHaveProperty("disabled", false)
    })
    await reloaded.type(
      await followUpComposer(reloaded),
      "Show the engine line.",
    )
    await reloaded.click(screen.getByRole("button", { name: "Send" }))
    expect(await screen.findByText("Engine line")).toBeTruthy()
    expect(screen.getByRole("heading", { name: "1. c3" })).toBeTruthy()
    expect(engineOpen.ply).toBe(initial.reviewMoment.ply)

    const commandsAfterHeal = await postedCommands(fetchMock)
    const healed = commandsAfterHeal.slice(commandsAfterReload.length)
    const hostTurns = healed.filter(
      ({ command }) => command.kind === "startHostTurn",
    )
    expect(hostTurns).toHaveLength(1)
    const inspected = healed.filter(
      ({ command }) =>
        command.kind === "inspectPosition" &&
        command.target.kind === "reviewedMove",
    )
    expect(inspected).toHaveLength(1)
    expect(inspected[0]?.command).toMatchObject({
      kind: "inspectPosition",
      reviewMomentId: initial.reviewMoment.momentId,
      target: { kind: "reviewedMove" },
    })
    expect(inspected[0]?.command).not.toMatchObject({
      reviewMomentId: later.reviewMoment.momentId,
    })
  })

  test("keeps a typed composer draft when an exploration commits", async () => {
    const fetchMock = reviewSessionFetch({
      alternativeScenario: "success",
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    const composer = await followUpComposer(user)
    await user.type(composer, "I am still typing this.")
    await user.click(screen.getByRole("gridcell", { name: /e2 white pawn/ }))
    await user.click(
      screen.getByRole("gridcell", { name: /e4 empty, legal destination/ }),
    )
    expect(screen.queryByText(/Stockfish evaluated/)).toBeNull()
    expect(await followUpComposer(user)).toHaveProperty(
      "value",
      "I am still typing this.",
    )

    expect(screen.getByLabelText("Explored alternatives")).toBeTruthy()
    await user.click(screen.getByRole("button", { name: "Exit branch" }))
    expect(screen.queryByLabelText("Explored alternatives")).toBeNull()
    expect(screen.queryByRole("button", { name: /Best move:/ })).toBeNull()
  })

  test("a failed inspection does not leave a badge for a missing branch", async () => {
    const alternativeMoveId = fromAlternativeMoveId(
      "alternative-move:web:missing-line",
    )
    const fetchMock = reviewSessionFetch({
      failInspection: true,
      hostTurn: {
        kind: "answer",
        showLine: { kind: "alternativeMove", alternativeMoveId },
      },
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    await screen.findByText(/My best guess is/)

    await user.type(await followUpComposer(user), "Show that alternative.")
    await user.click(screen.getByRole("button", { name: "Send" }))

    expect(await screen.findByText("The knight was hanging.")).toBeTruthy()
    await waitFor(async () => {
      expect(
        (await postedCommands(fetchMock)).some(
          ({ command }) =>
            command.kind === "inspectPosition" &&
            command.target.kind === "alternativeMove" &&
            command.target.alternativeMoveId === alternativeMoveId,
        ),
      ).toBe(true)
    })
    expect(screen.queryByText("Alternative branch")).toBeNull()
  })

  test("keeps a mixed grounded review in Game order through navigation and plan discussion", async () => {
    const initial = fixtureCore()
    const later = preparedCoreAtPly(3)
    const review = reviewWithLazyMoment(
      fixtureGameReview(),
      initial.importedGame,
    )
    const improvement = review.criticalMoments[1]
    if (!improvement) throw new Error("mixed fixture requires a later moment")
    improvement.classification = {
      kind: "improvementOpportunity",
      correction: {
        betterMoveUci: "b1c3",
        betterMoveSan: "Nc3",
        outcome: {
          kind: "improvedAnalyzed",
          betterEvaluation: {
            kind: "centipawns",
            value: 35,
            perspective: "white",
          },
        },
      },
    }
    improvement.comment = {
      text: "After Nf3, Nc3 was the grounded improvement because it develops with more pressure. My best guess is that Nf3 was meant to prepare castling. Was that your idea, or did you have another plan?",
    }
    const fetchMock = reviewSessionFetch({
      preparedReviewMoments: [initial, later],
      review,
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()

    await screen.findByRole("main", { name: "Game review" })
    const picker = screen.getByLabelText("Full game move list")
    expect(
      within(picker).getByRole("button", { name: /Positive highlight/ }),
    ).toBeTruthy()
    expect(
      within(picker).getByRole("button", { name: /Improvement opportunity/ }),
    ).toBeTruthy()
    expect(screen.queryByText("Practice options")).toBeNull()
    expect(screen.queryByRole("link", { name: /lesson/i })).toBeNull()
    expect(
      await screen.findByText(/After c3, compare the reported 0.0/),
    ).toBeTruthy()
    expect(
      screen.getAllByLabelText("Coaching moment").length,
    ).toBeGreaterThanOrEqual(2)
    await user.click(within(picker).getByRole("button", { name: /2\. d4/ }))

    expect(await screen.findByRole("heading", { name: "2. d4" })).toBeTruthy()
    expect(
      await screen.findByText(/After Nf3, Nc3 was the grounded improvement/),
    ).toBeTruthy()
    const discussion = "I wanted to prepare castling before opening the center."
    await user.type(await followUpComposer(user), discussion)
    await user.click(screen.getByRole("button", { name: "Send" }))
    expect(screen.getByText(discussion)).toBeTruthy()
    expect(await screen.findByText("The knight was hanging.")).toBeTruthy()
    const firstMomentButton = within(picker)
      .getByText("1. c3")
      .closest("button")
    if (!firstMomentButton)
      throw new Error("the first Review Moment must be selectable")
    await user.click(firstMomentButton)
    expect(await screen.findByRole("heading", { name: "1. c3" })).toBeTruthy()
    expect(
      await screen.findByText(/After c3, compare the reported 0.0/),
    ).toBeTruthy()
    await waitFor(async () => {
      expect(
        (await postedCommands(fetchMock)).map(
          (command) => command.command.kind,
        ),
      ).toEqual(["startReviewSession", "openReviewMoment", "startHostTurn"])
    })
  })

  test("shows the frozen comment without opening a Review Moment", async () => {
    const firstOpen: ReviewMomentOpenHold = {}
    const fetchMock = reviewSessionFetch({ holdFirstOpen: firstOpen })
    vi.stubGlobal("fetch", fetchMock)
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    expect(await screen.findByText(/My best guess is/)).toBeTruthy()
    expect(document.querySelector("[data-comment-wait=bounded]")).toBeNull()
    expect(
      (await postedCommands(fetchMock)).map(({ command }) => command.kind),
    ).toEqual(["startReviewSession"])
    expect(firstOpen.release).toBeUndefined()
  })

  test("opens the Review Moment only when the Player sends a message", async () => {
    const firstOpen: ReviewMomentOpenHold = {}
    const fetchMock = reviewSessionFetch({ holdFirstOpen: firstOpen })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    expect(await screen.findByText(/My best guess is/)).toBeTruthy()

    await user.type(await followUpComposer(user), "Why this move?")
    await user.click(screen.getByRole("button", { name: "Send" }))
    expect(screen.getByText("Why this move?")).toBeTruthy()
    expect(firstOpen.release).toEqual(expect.any(Function))
    firstOpen.release?.()
    expect(await screen.findByText("The knight was hanging.")).toBeTruthy()
    expect(
      (await postedCommands(fetchMock)).map(({ command }) => command.kind),
    ).toEqual(["startReviewSession", "openReviewMoment", "startHostTurn"])
  })

  test("opens a legal Player-selected Moment from a zero-moment review", async () => {
    const fetchMock = reviewSessionFetch({ preparedReviewMoments: [] })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()

    expect(await screen.findByText("No key moments found")).toBeTruthy()
    expect(screen.getByLabelText("Evaluation timeline")).toBeTruthy()
    await user.click(
      within(screen.getByLabelText("Full game move list")).getByRole("button", {
        name: "2. d4",
      }),
    )

    expect(await screen.findByRole("heading", { name: "2. d4" })).toBeTruthy()
    const opening = screen.getByText(/Neutral: Nf3\./)
    expect(opening.textContent).toContain(
      "Verified observation: White played Nf3 at ply 3.",
    )
    expect(opening.textContent).toContain(
      "Intent analysis does not apply to Nf3 because it is outside your Review Side.",
    )
    expect(
      screen.queryByRole("region", {
        name: "Learning plan for this moment",
      }),
    ).toBeNull()
    expect(
      (await postedCommands(fetchMock)).map((command) => command.command.kind),
    ).toEqual(["startReviewSession", "openReviewMoment"])
  })

  test("renders canonical links from Player-selected moment-local material", async () => {
    const selected = preparedCoreAtPly(3)
    const localMaterial = forkLearningMaterial(
      selected.reviewMoment.momentId,
      selected.reviewMoment.ply,
    )
    const fetchMock = reviewSessionFetch({
      playerSelectedLearningMaterial: localMaterial,
      preparedReviewMoments: [],
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()

    await screen.findByText("No key moments found")
    await user.click(
      within(screen.getByLabelText("Full game move list")).getByRole("button", {
        name: "2. d4",
      }),
    )

    expect(
      (
        await screen.findByRole("link", {
          name: /Concept lesson: The Fork/,
        })
      ).getAttribute("href"),
    ).toBe("https://lichess.org/practice/fundamental-tactics/the-fork/Qj281y1p")
    expect(
      screen
        .getByRole("link", { name: /Pattern drilling: Fork/ })
        .getAttribute("href"),
    ).toBe("https://lichess.org/training/fork")
  })

  test("player-selected ply starts the bounded wait before openReviewMoment returns", async () => {
    const playerOpen: ReviewMomentOpenHold = {}
    const fetchMock = reviewSessionFetch({
      holdPlayerSelectedOpen: playerOpen,
      preparedReviewMoments: [],
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()

    expect(await screen.findByText("No key moments found")).toBeTruthy()
    await user.click(
      within(screen.getByLabelText("Full game move list")).getByRole("button", {
        name: "2. d4",
      }),
    )

    await waitFor(() => {
      expect(document.querySelector("[data-comment-wait=bounded]")).toBeTruthy()
    })
    expect(screen.queryByText(/Neutral: Nf3/)).toBeNull()
    playerOpen.release?.()
    expect(await screen.findByText(/Neutral: Nf3./)).toBeTruthy()
    expect(document.querySelector("[data-comment-wait=bounded]")).toBeNull()
  })

  test("walking a ply from an open session does not open a Player-Selected Moment", async () => {
    const fetchMock = reviewSessionFetch()
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    expect(await screen.findByText(/My best guess is/)).toBeTruthy()
    expect(screen.queryByText("Discuss this position?")).toBeNull()
    const kindsBefore = (await postedCommands(fetchMock)).map(
      ({ command }) => command.kind,
    )
    await user.click(
      within(screen.getByLabelText("Full game move list")).getByRole("button", {
        name: "2. d4",
      }),
    )
    expect(screen.getByText("Discuss this position?")).toBeTruthy()
    expect(
      (await postedCommands(fetchMock)).map(({ command }) => command.kind),
    ).toEqual(kindsBefore)
    expect(screen.queryByText(/Neutral: Nf3/)).toBeNull()
    // The walked position is not a Critical Moment, so the moment's
    // commentary yields to the neutral prompt.
    expect(screen.queryByText(/My best guess is/)).toBeNull()
  })

  test("nominating a walked ply opens a Player-Selected Moment", async () => {
    const playerOpen: ReviewMomentOpenHold = {}
    const fetchMock = reviewSessionFetch({
      holdPlayerSelectedOpen: playerOpen,
    })
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    renderWorkspace()
    await screen.findByRole("main", { name: "Game review" })
    expect(await screen.findByText(/My best guess is/)).toBeTruthy()
    await user.click(
      within(screen.getByLabelText("Full game move list")).getByRole("button", {
        name: "2. d4",
      }),
    )
    await user.type(
      await followUpComposer(user),
      "What should I plan from here?",
    )
    await user.click(screen.getByRole("button", { name: "Send" }))
    await waitFor(() => {
      expect(document.querySelector("[data-comment-wait=bounded]")).toBeTruthy()
    })
    expect(screen.queryByText(/Neutral: Nf3/)).toBeNull()
    playerOpen.release?.()
    expect(await screen.findByText(/Neutral: Nf3./)).toBeTruthy()
    expect(document.querySelector("[data-comment-wait=bounded]")).toBeNull()
  })
})

async function followUpComposer(_user: ReturnType<typeof userEvent.setup>) {
  return screen.getByRole("textbox", { name: "Message the coach" })
}

/** A workspace is always addressed to a Game Review: that is its only entry. */
function renderWorkspace({
  initialGameImportId = FIXTURE_GAME_IMPORT_ID,
  reauthenticate = async () => undefined,
  signedInAs = "player@example.com",
  signOut = async () => undefined,
}: {
  initialGameImportId?: GameImportId
  reauthenticate?: (password: string) => Promise<void>
  signedInAs?: string
  signOut?: () => Promise<void>
} = {}) {
  render(
    <CoachWorkspace
      fetchAccessToken={async () => "review-jwt"}
      initialPly={null}
      initialGameImportId={initialGameImportId}
      reauthenticate={reauthenticate}
      signedInAs={signedInAs}
      signOut={signOut}
    />,
  )
}

function verifiedIdentity(): Extract<MockIdentity, { kind: "signedIn" }> {
  return {
    kind: "signedIn",
    email: "player@example.com",
    emailVerified: true,
    playerId: "firebase-player-test",
  }
}

/** The shared ndjson responder, wrapped so tests can read the posted commands. */
function reviewSessionFetch(options: ReviewSessionFixtureOptions = {}) {
  return vi
    .fn<typeof fetch>()
    .mockImplementation(reviewSessionResponder(options))
}

async function postedCommands(
  fetchMock: ReturnType<typeof reviewSessionFetch>,
) {
  return Promise.all(
    fetchMock.mock.calls
      .filter(([input]) =>
        String(input).endsWith("/api/v1/review-session/commands"),
      )
      .map(([, init]) =>
        decodeReviewSessionCommandEnvelope(
          JSON.parse(String(init?.body)) as unknown,
        ),
      ),
  )
}

function accountSettingsRoot() {
  const settings = document.querySelector("[data-account-settings]")
  if (!(settings instanceof HTMLElement)) {
    throw new Error("expected Account Settings")
  }
  return settings
}

/** The watercolor checkbox hides its input behind the painted mark, so a
 * pointer reaches it through the wrapping label. */
function checkboxTarget(input: HTMLElement) {
  const label = input.closest("label")
  if (!label) throw new Error("expected a labelled checkbox")
  return label
}

function accountSettingsQualityCapturePreference() {
  return within(accountSettingsRoot()).getByRole("checkbox", {
    name: "Help improve coaching",
  })
}

function preferenceUpdates(fetchMock: ReturnType<typeof reviewSessionFetch>) {
  return fetchMock.mock.calls
    .filter(
      ([input, init]) =>
        String(input).endsWith("/api/v1/review-artifacts/preference") &&
        init?.method === "PUT",
    )
    .map(([, init]) => parseEnabledPreference(init?.body))
}

async function waitForCommandCount(
  fetchMock: ReturnType<typeof reviewSessionFetch>,
  count: number,
) {
  await vi.waitFor(async () =>
    expect(await postedCommands(fetchMock)).toHaveLength(count),
  )
}
