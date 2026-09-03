// @vitest-environment jsdom

import { act, cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { FirebaseError } from "firebase/app"
import { useEffect, type ReactNode } from "react"
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest"

import { BetaAccessBoundary } from "./BetaAccessBoundary"
import {
  FirebaseAuthProvider,
  useFirebaseAuth,
  type FirebaseAuthApi,
  type FirebaseAuthCredential,
  type FirebaseAuthHandle,
  type FirebaseAuthUser,
} from "./FirebaseAuthProvider"
import { LoginIdentity } from "./LoginIdentity"
import {
  coachingBoardDestination,
  type VerifiedIdentityDestination,
} from "./verifiedIdentityDestination"

import { type JsonObject } from "@chenchess/coach-engine-sdk"
type FirebaseAuthDoubles = {
  createUser: ReturnType<typeof vi.fn>
  getIdToken: ReturnType<typeof vi.fn>
  googleCredentialFromError: ReturnType<typeof vi.fn>
  googleCustomParameters: ReturnType<typeof vi.fn>
  link: ReturnType<typeof vi.fn>
  listener: ((user: FirebaseAuthUser | null) => void) | null
  reauthenticate: ReturnType<typeof vi.fn>
  reload: ReturnType<typeof vi.fn>
  resetPassword: ReturnType<typeof vi.fn>
  sendVerification: ReturnType<typeof vi.fn>
  signIn: ReturnType<typeof vi.fn>
  signInWithPopup: ReturnType<typeof vi.fn>
  signOut: ReturnType<typeof vi.fn>
}

const firebase = vi.hoisted(() => {
  const doubles: FirebaseAuthDoubles = {
    createUser: vi.fn(),
    getIdToken: vi.fn(),
    googleCredentialFromError: vi.fn(),
    googleCustomParameters: vi.fn(),
    link: vi.fn(),
    listener: null,
    reauthenticate: vi.fn(),
    reload: vi.fn(),
    resetPassword: vi.fn(),
    sendVerification: vi.fn(),
    signIn: vi.fn(),
    signInWithPopup: vi.fn(),
    signOut: vi.fn(),
  }
  return doubles
})

const oauthDestination: VerifiedIdentityDestination = {
  href: "/interaction/oauth_interaction_123",
  joinHref: "/join/?oauth_interaction=oauth_interaction_123",
  loginHref: "/login/?oauth_interaction=oauth_interaction_123",
  requiresBetaAccess: true,
}

const authApi: FirebaseAuthApi = {
  createUserWithEmailAndPassword: firebase.createUser,
  EmailAuthProvider: {
    credential: (email: string, password: string) => ({
      email,
      password,
      providerId: "password",
      signInMethod: "password",
    }),
  },
  getIdToken: firebase.getIdToken,
  GoogleAuthProvider: class GoogleAuthProvider {
    static credentialFromError(error: unknown) {
      return firebase.googleCredentialFromError(error)
    }

    setCustomParameters(parameters: Record<string, string>) {
      firebase.googleCustomParameters(parameters)
    }
  },
  linkWithCredential: firebase.link,
  onIdTokenChanged: (
    _auth: FirebaseAuthHandle,
    listener: (user: FirebaseAuthUser | null) => void,
  ) => {
    firebase.listener = listener
    return vi.fn()
  },
  reauthenticateWithCredential: firebase.reauthenticate,
  reload: firebase.reload,
  sendEmailVerification: firebase.sendVerification,
  sendPasswordResetEmail: firebase.resetPassword,
  signInWithEmailAndPassword: firebase.signIn,
  signInWithPopup: firebase.signInWithPopup,
  signOut: firebase.signOut,
}

beforeEach(() => {
  firebase.createUser.mockReset()
  firebase.getIdToken.mockReset().mockResolvedValue("firebase-id-token")
  firebase.googleCredentialFromError.mockReset()
  firebase.googleCustomParameters.mockReset()
  firebase.link.mockReset().mockResolvedValue(undefined)
  firebase.listener = null
  firebase.reauthenticate.mockReset().mockResolvedValue(undefined)
  firebase.reload.mockReset().mockResolvedValue(undefined)
  firebase.resetPassword.mockReset().mockResolvedValue(undefined)
  firebase.sendVerification.mockReset().mockResolvedValue(undefined)
  firebase.signIn.mockReset()
  firebase.signInWithPopup.mockReset()
  firebase.signOut.mockReset().mockResolvedValue(undefined)
  vi.stubGlobal(
    "fetch",
    vi
      .fn<typeof fetch>()
      .mockResolvedValue(new Response(null, { status: 403 })),
  )
})

afterEach(() => {
  cleanup()
  // The Beta Access grant cache is tab-scoped global state; without this a
  // granted test leaves the next one skipping the checking gate.
  sessionStorage.clear()
  vi.unstubAllGlobals()
})

describe("Firebase identity journeys", () => {
  test("signs up with password, sends verification, and blocks until refreshed", async () => {
    const auth = firebaseAuth()
    const unverified = firebaseUser({ emailVerified: false })
    firebase.createUser.mockResolvedValue({ user: unverified })
    const user = userEvent.setup()
    const navigate = vi.fn()
    renderJourney(auth, coachingBoardDestination, navigate)
    emitIdentity(auth, null)

    await user.click(screen.getByRole("button", { name: "Create account" }))
    await enterCredentials(user)
    await user.click(screen.getByRole("button", { name: "Create account" }))

    expect(firebase.createUser).toHaveBeenCalledWith(
      auth,
      "player@example.test",
      "password123",
    )
    expect(firebase.sendVerification).toHaveBeenCalledWith(unverified, {
      url: `${window.location.origin}/login/`,
    })

    emitIdentity(auth, unverified)
    expect(
      await screen.findByRole("heading", { name: "Verify your email" }),
    ).toBeTruthy()
    expect(screen.getByText(/redeem an invite once/i)).toBeTruthy()

    await user.click(
      screen.getByRole("button", {
        name: "Send another verification email",
      }),
    )
    expect(firebase.sendVerification).toHaveBeenLastCalledWith(unverified, {
      url: `${window.location.origin}/login/`,
    })

    const verified = firebaseUser({ emailVerified: true })
    setCurrentUser(auth, verified)
    await user.click(
      screen.getByRole("button", { name: "I verified—check again" }),
    )

    expect(firebase.reload).toHaveBeenCalledWith(verified)
    expect(firebase.getIdToken).toHaveBeenCalledWith(verified, true)
    await waitFor(() =>
      expect(navigate).toHaveBeenCalledWith(coachingBoardDestination.joinHref),
    )
  })

  test("resets generically without losing the OAuth return", async () => {
    const auth = firebaseAuth()
    const navigate = vi.fn()
    const user = userEvent.setup()
    renderJourney(auth, oauthDestination, navigate)
    emitIdentity(auth, null)

    await requestPasswordReset(user, "known@example.test")
    const genericMessage =
      "If an account can receive a reset email, instructions are on the way."
    expect(screen.getByText(genericMessage)).toBeTruthy()

    await user.click(
      screen.getByRole("button", { name: "Use an existing account" }),
    )
    firebase.resetPassword.mockRejectedValueOnce(
      firebaseError("auth/user-not-found", "Private account detail"),
    )
    await requestPasswordReset(user, "unknown@example.test")

    expect(screen.getByText(genericMessage)).toBeTruthy()
    expect(screen.queryByText("Private account detail")).toBeNull()
    expect(firebase.resetPassword).toHaveBeenNthCalledWith(
      1,
      auth,
      "known@example.test",
      { url: `${window.location.origin}/login/` },
    )
    expect(firebase.resetPassword).toHaveBeenNthCalledWith(
      2,
      auth,
      "unknown@example.test",
      { url: `${window.location.origin}/login/` },
    )

    const passwordUser = firebaseUser({ providerIds: ["password"] })
    firebase.signIn.mockResolvedValue({ user: passwordUser })
    await user.click(
      screen.getByRole("button", { name: "Use an existing account" }),
    )
    await enterCredentials(user)
    await user.click(screen.getByRole("button", { name: "Sign in" }))
    emitIdentity(auth, passwordUser)
    await waitFor(() =>
      expect(navigate).toHaveBeenCalledWith(oauthDestination.joinHref),
    )
  })

  test("signs in with email and password without exposing Firebase failures", async () => {
    const auth = firebaseAuth()
    const passwordUser = firebaseUser({ providerIds: ["password"] })
    firebase.signIn.mockResolvedValue({ user: passwordUser })
    const user = userEvent.setup()
    const navigate = vi.fn()
    renderJourney(auth, coachingBoardDestination, navigate)
    emitIdentity(auth, null)

    await enterCredentials(user)
    await user.click(screen.getByRole("button", { name: "Sign in" }))

    expect(firebase.signIn).toHaveBeenCalledWith(
      auth,
      "player@example.test",
      "password123",
    )
    emitIdentity(auth, passwordUser)
    await waitFor(() =>
      expect(navigate).toHaveBeenCalledWith(coachingBoardDestination.joinHref),
    )
  })

  test("keeps sign-out available while login checks Beta Access", async () => {
    const auth = firebaseAuth()
    vi.stubGlobal(
      "fetch",
      vi
        .fn<typeof fetch>()
        .mockImplementation(() => new Promise<Response>(() => undefined)),
    )
    const user = userEvent.setup()
    renderJourney(auth)

    emitIdentity(auth, firebaseUser())

    expect(
      await screen.findByRole("heading", { name: "Checking your access" }),
    ).toBeTruthy()
    await user.click(screen.getByRole("button", { name: "Log out" }))
    expect(firebase.signOut).toHaveBeenCalledWith(auth)
  })

  test("does not repeat Beta Access authorization after reading its Firebase token", async () => {
    const auth = firebaseAuth()
    const signedIn = firebaseUser()
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockImplementation(async () => Response.json({ playerId: signedIn.uid }))
    vi.stubGlobal("fetch", fetchMock)

    render(
      <FirebaseAuthProvider auth={auth} authApi={authApi}>
        <BetaAccessProbe />
      </FirebaseAuthProvider>,
    )
    emitIdentity(auth, signedIn)

    expect(await screen.findByText("Authorized application")).toBeTruthy()
    await waitFor(() => expect(fetchMock).toHaveBeenCalledOnce())
    expect(firebase.getIdToken).toHaveBeenCalledWith(signedIn, false)
  })

  // The authorized page reads Coach Engine with a forced token refresh, so this
  // is the ordinary dashboard load: remounting it would start the next refresh
  // and never stop.
  test("keeps the authorized page mounted while it forces token refreshes", async () => {
    const auth = firebaseAuth()
    const signedIn = firebaseUser()
    let issuedTokens = 0
    firebase.getIdToken.mockImplementation(
      async (user: FirebaseAuthUser, forceRefreshToken: boolean) => {
        if (!forceRefreshToken) return "firebase-id-token"
        issuedTokens += 1
        // Firebase notifies id-token listeners whenever a refresh mints a token.
        queueMicrotask(() => firebase.listener?.(user))
        return `firebase-id-token-${issuedTokens}`
      },
    )
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockImplementation(async () => Response.json({ playerId: signedIn.uid }))
    vi.stubGlobal("fetch", fetchMock)
    const mounted = vi.fn()

    render(
      <FirebaseAuthProvider auth={auth} authApi={authApi}>
        <BetaAccessProbe authorized={<CoachEngineReader onMount={mounted} />} />
      </FirebaseAuthProvider>,
    )
    emitIdentity(auth, signedIn)

    expect(await screen.findByText("Authorized application")).toBeTruthy()
    await settleMicrotasks()

    expect(mounted).toHaveBeenCalledOnce()
    expect(issuedTokens).toBe(1)
    expect(fetchMock).toHaveBeenCalledOnce()
  })

  test("keeps the verification UI stable when token refresh fails", async () => {
    const auth = firebaseAuth()
    const unverified = firebaseUser({ emailVerified: false })
    firebase.getIdToken.mockRejectedValue(
      firebaseError("auth/network-request-failed", "Private network detail"),
    )
    const user = userEvent.setup()
    renderJourney(auth)
    emitIdentity(auth, unverified)

    await user.click(
      screen.getByRole("button", { name: "I verified—check again" }),
    )

    expect(
      screen.getByRole("heading", { name: "Verify your email" }),
    ).toBeTruthy()
    expect(screen.getByText(/temporarily unavailable/i)).toBeTruthy()
    expect(screen.queryByText("Private network detail")).toBeNull()
  })

  test.each([
    ["auth/popup-closed-by-user", "Sign-in was canceled"],
    ["auth/redirect-cancelled-by-user", "Sign-in was canceled"],
    ["auth/popup-blocked", "Sign-in is temporarily unavailable"],
    ["auth/too-many-requests", "Too many attempts"],
  ])("keeps a stable, private UI after %s", async (code, safeMessage) => {
    const auth = firebaseAuth()
    firebase.signInWithPopup.mockRejectedValue(
      firebaseError(code, "Sensitive Firebase provider detail"),
    )
    const user = userEvent.setup()
    renderJourney(auth)
    emitIdentity(auth, null)

    await user.click(
      screen.getByRole("button", { name: "Continue with Google" }),
    )

    expect(screen.getByText(new RegExp(safeMessage))).toBeTruthy()
    expect(screen.queryByText("Sensitive Firebase provider detail")).toBeNull()
    expect(screen.getByRole("heading", { name: "Sign in" })).toBeTruthy()
  })

  test("signs in with configured Google parameters and signs out cleanly", async () => {
    const auth = firebaseAuth()
    const googleUser = firebaseUser({
      emailVerified: false,
      providerIds: ["google.com"],
    })
    firebase.signInWithPopup.mockResolvedValue({ user: googleUser })
    const user = userEvent.setup()
    renderJourney(auth)
    emitIdentity(auth, null)

    await user.click(
      screen.getByRole("button", { name: "Continue with Google" }),
    )
    emitIdentity(auth, googleUser)

    expect(firebase.googleCustomParameters).toHaveBeenCalledWith({
      prompt: "select_account",
    })
    expect(
      await screen.findByRole("heading", { name: "Verify your email" }),
    ).toBeTruthy()

    await user.click(screen.getByRole("button", { name: "Log out" }))
    expect(firebase.signOut).toHaveBeenCalledWith(auth)
    emitIdentity(auth, null)
    expect(await screen.findByRole("heading", { name: "Sign in" })).toBeTruthy()
  })

  test("keeps an authenticated UI when sign-out fails", async () => {
    const auth = firebaseAuth()
    const signedIn = firebaseUser({ emailVerified: false })
    firebase.signOut.mockRejectedValue(
      firebaseError("auth/network-request-failed", "Private sign-out detail"),
    )
    const user = userEvent.setup()
    renderJourney(auth)
    emitIdentity(auth, signedIn)

    await user.click(screen.getByRole("button", { name: "Log out" }))

    expect(
      screen.getByRole("heading", { name: "Verify your email" }),
    ).toBeTruthy()
    expect(screen.getByText(/temporarily unavailable/i)).toBeTruthy()
    expect(screen.queryByText("Private sign-out detail")).toBeNull()
  })

  test("authenticates the password account before linking a pending Google credential", async () => {
    const auth = firebaseAuth()
    const googleCredential = credential("google.com")
    const collision = firebaseError(
      "auth/account-exists-with-different-credential",
      "Do not expose provider inventory",
      { email: "Player@Example.Test" },
    )
    firebase.googleCredentialFromError.mockReturnValue(googleCredential)
    firebase.signInWithPopup.mockRejectedValue(collision)
    const passwordUser = firebaseUser({ providerIds: ["password"] })
    firebase.signIn.mockResolvedValue({ user: passwordUser })
    const navigate = vi.fn()
    const user = userEvent.setup()
    renderJourney(auth, oauthDestination, navigate)
    emitIdentity(auth, null)

    await user.click(
      screen.getByRole("button", { name: "Continue with Google" }),
    )

    expect(
      await screen.findByRole("heading", { name: "Confirm before linking" }),
    ).toBeTruthy()
    expect(firebase.link).not.toHaveBeenCalled()
    expect(screen.queryByText("Do not expose provider inventory")).toBeNull()

    await user.type(screen.getByLabelText("Password"), "password123")
    await user.click(
      screen.getByRole("button", { name: "Sign in and link accounts" }),
    )

    expect(firebase.signIn).toHaveBeenCalledWith(
      auth,
      "player@example.test",
      "password123",
    )
    expect(firebase.link).toHaveBeenCalledWith(passwordUser, googleCredential)
    emitIdentity(auth, passwordUser)
    await waitFor(() =>
      expect(navigate).toHaveBeenCalledWith(oauthDestination.joinHref),
    )
  })

  test("authenticates the Google account before linking a pending password credential", async () => {
    const auth = firebaseAuth()
    firebase.createUser.mockRejectedValue(
      firebaseError("auth/email-already-in-use", "Private provider detail"),
    )
    const googleUser = firebaseUser({ providerIds: ["google.com"] })
    firebase.signInWithPopup.mockResolvedValue({ user: googleUser })
    const user = userEvent.setup()
    renderJourney(auth)
    emitIdentity(auth, null)

    await user.click(screen.getByRole("button", { name: "Create account" }))
    await enterCredentials(user)
    await user.click(screen.getByRole("button", { name: "Create account" }))

    expect(firebase.link).not.toHaveBeenCalled()
    await user.click(
      screen.getByRole("button", {
        name: "Continue with Google and link",
      }),
    )

    expect(firebase.signInWithPopup).toHaveBeenCalledWith(
      auth,
      expect.anything(),
    )
    expect(firebase.link).toHaveBeenCalledWith(
      googleUser,
      expect.objectContaining({
        email: "player@example.test",
        password: "password123",
        providerId: "password",
      }),
    )
  })

  test("never links matching email text when authentication returns another account", async () => {
    const auth = firebaseAuth()
    const googleCredential = credential("google.com")
    firebase.googleCredentialFromError.mockReturnValue(googleCredential)
    firebase.signInWithPopup.mockRejectedValueOnce(
      firebaseError(
        "auth/account-exists-with-different-credential",
        "Private provider detail",
        { email: "player@example.test" },
      ),
    )
    const otherUser = firebaseUser({
      email: "other@example.test",
      providerIds: ["password"],
    })
    firebase.signIn.mockResolvedValue({ user: otherUser })
    const user = userEvent.setup()
    renderJourney(auth)
    emitIdentity(auth, null)

    await user.click(
      screen.getByRole("button", { name: "Continue with Google" }),
    )
    await user.type(screen.getByLabelText("Password"), "password123")
    await user.click(
      screen.getByRole("button", { name: "Sign in and link accounts" }),
    )

    expect(firebase.link).not.toHaveBeenCalled()
    expect(firebase.signOut).toHaveBeenCalledWith(auth)
    expect(
      screen.getByText(/different account.*No accounts were linked/i),
    ).toBeTruthy()

    await user.click(screen.getByRole("button", { name: "Cancel linking" }))
    expect(await screen.findByRole("heading", { name: "Sign in" })).toBeTruthy()
  })

  test("does not relink a credential the authenticated account already owns", async () => {
    const auth = firebaseAuth()
    firebase.createUser.mockRejectedValue(
      firebaseError("auth/email-already-in-use", "Private provider detail"),
    )
    const passwordUser = firebaseUser({ providerIds: ["password"] })
    firebase.signIn.mockResolvedValue({ user: passwordUser })
    const user = userEvent.setup()
    renderJourney(auth)
    emitIdentity(auth, null)

    await user.click(screen.getByRole("button", { name: "Create account" }))
    await enterCredentials(user)
    await user.click(screen.getByRole("button", { name: "Create account" }))
    await user.type(screen.getByLabelText("Password"), "password123")
    await user.click(
      screen.getByRole("button", { name: "Sign in and link accounts" }),
    )

    expect(firebase.signIn).toHaveBeenCalled()
    expect(firebase.link).not.toHaveBeenCalled()
    expect(
      screen.queryByRole("heading", { name: "Confirm before linking" }),
    ).toBeNull()
  })
})

function renderJourney(
  auth: FirebaseAuthHandle,
  verifiedDestination: VerifiedIdentityDestination = coachingBoardDestination,
  navigate = vi.fn(),
) {
  render(
    <FirebaseAuthProvider auth={auth} authApi={authApi}>
      <LoginIdentity
        initialInvitationCode={null}
        navigate={navigate}
        verifiedDestination={verifiedDestination}
      />
    </FirebaseAuthProvider>,
  )
}

function BetaAccessProbe({ authorized }: { authorized?: ReactNode }) {
  const { fetchAccessToken, identity } = useFirebaseAuth()
  if (identity.kind !== "signedIn") return null
  return (
    <BetaAccessBoundary
      destination={coachingBoardDestination}
      fetchAccessToken={fetchAccessToken}
      identity={identity}
      navigate={vi.fn()}
      signOut={vi.fn().mockResolvedValue(undefined)}
    >
      {() => authorized ?? <p>Authorized application</p>}
    </BetaAccessBoundary>
  )
}

/** Stands in for the dashboard: reads Coach Engine with a fresh token. */
function CoachEngineReader({ onMount }: { onMount: () => void }) {
  const { fetchAccessToken } = useFirebaseAuth()
  useEffect(() => {
    onMount()
    void fetchAccessToken({ forceRefreshToken: true })
  }, [fetchAccessToken, onMount])
  return <p>Authorized application</p>
}

async function settleMicrotasks() {
  await act(async () => {
    for (let turn = 0; turn < 20; turn += 1) await Promise.resolve()
  })
}

function firebaseAuth(): FirebaseAuthHandle {
  return { currentUser: null }
}

function firebaseUser({
  email = "player@example.test",
  emailVerified = true,
  providerIds = ["password"],
}: {
  email?: string
  emailVerified?: boolean
  providerIds?: string[]
} = {}): FirebaseAuthUser {
  return {
    email,
    emailVerified,
    providerData: providerIds.map((providerId) => ({ providerId })),
    uid: "firebase-player",
  }
}

function credential(providerId: string): FirebaseAuthCredential {
  return { providerId, signInMethod: providerId }
}

function setCurrentUser(
  auth: FirebaseAuthHandle,
  user: FirebaseAuthUser | null,
) {
  Object.assign(auth, { currentUser: user })
}

function emitIdentity(auth: FirebaseAuthHandle, user: FirebaseAuthUser | null) {
  act(() => {
    setCurrentUser(auth, user)
    firebase.listener?.(user)
  })
}

async function enterCredentials(
  user: ReturnType<typeof userEvent.setup>,
  email = "player@example.test",
) {
  await user.type(screen.getByLabelText("Email"), email)
  await user.type(screen.getByLabelText("Password"), "password123")
}

async function requestPasswordReset(
  user: ReturnType<typeof userEvent.setup>,
  email: string,
) {
  await user.click(screen.getByRole("button", { name: "Forgot password?" }))
  await user.type(screen.getByLabelText("Email"), email)
  await user.click(
    screen.getByRole("button", { name: "Send reset instructions" }),
  )
}

function firebaseError(code: string, message: string, customData?: JsonObject) {
  return new FirebaseError(code, message, customData)
}
