import { FirebaseError } from "firebase/app"
import {
  createUserWithEmailAndPassword,
  EmailAuthProvider,
  getIdToken,
  GoogleAuthProvider,
  linkWithCredential,
  onIdTokenChanged,
  reauthenticateWithCredential,
  reload,
  sendEmailVerification,
  sendPasswordResetEmail,
  signInWithEmailAndPassword,
  signInWithPopup,
  signOut as firebaseSignOut,
  type Auth,
  type ActionCodeSettings,
  type AuthCredential,
  type User,
} from "firebase/auth"
import * as v from "valibot"
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type PropsWithChildren,
} from "react"

import { purgeReviewSnapshotCache } from "../game-review/reviewSnapshotCache"
export type FetchAccessToken = (options: {
  forceRefreshToken: boolean
}) => Promise<string | null>

export type FirebaseIdentity =
  | { kind: "loading" }
  | { kind: "signedOut" }
  | {
      kind: "signedIn"
      email: string | null
      emailVerified: boolean
      playerId: string
    }

export type IdentityJourneyResult =
  | { kind: "complete" }
  | { kind: "linkRequired" }
  | { kind: "wrongAccount" }
  | { kind: "canceled" }
  | { kind: "invalidCredentials" }
  | { kind: "tryLater" }
  | { kind: "unavailable" }

export type ProviderLinkState =
  | { kind: "none" }
  | { kind: "required"; email: string }

export type FirebaseAuthUser = {
  email: string | null
  emailVerified: boolean
  providerData: ReadonlyArray<{ providerId: string }>
  uid: string
}

export type FirebaseAuthHandle = {
  currentUser: FirebaseAuthUser | null
}

export type FirebaseAuthCredential = {
  providerId: string
  signInMethod: string
}

export type GoogleAuthCustomParameters = {
  readonly [key: string]: string
}

type GoogleAuthProviderInstance = {
  setCustomParameters: (parameters: GoogleAuthCustomParameters) => void
}

export type FirebaseAuthApi = {
  EmailAuthProvider: {
    credential: (email: string, password: string) => FirebaseAuthCredential
  }
  GoogleAuthProvider: {
    new (): GoogleAuthProviderInstance
    credentialFromError: (error: unknown) => FirebaseAuthCredential | null
  }
  createUserWithEmailAndPassword: (
    auth: FirebaseAuthHandle,
    email: string,
    password: string,
  ) => Promise<{ user: FirebaseAuthUser }>
  getIdToken: (
    user: FirebaseAuthUser,
    forceRefresh?: boolean,
  ) => Promise<string>
  linkWithCredential: (
    user: FirebaseAuthUser,
    credential: FirebaseAuthCredential,
  ) => Promise<void>
  onIdTokenChanged: (
    auth: FirebaseAuthHandle,
    listener: (user: FirebaseAuthUser | null) => void,
  ) => () => void
  reauthenticateWithCredential: (
    user: FirebaseAuthUser,
    credential: FirebaseAuthCredential,
  ) => Promise<void>
  reload: (user: FirebaseAuthUser) => Promise<void>
  sendEmailVerification: (
    user: FirebaseAuthUser,
    settings?: ActionCodeSettings,
  ) => Promise<void>
  sendPasswordResetEmail: (
    auth: FirebaseAuthHandle,
    email: string,
    settings?: ActionCodeSettings,
  ) => Promise<void>
  signInWithEmailAndPassword: (
    auth: FirebaseAuthHandle,
    email: string,
    password: string,
  ) => Promise<{ user: FirebaseAuthUser }>
  signInWithPopup: (
    auth: FirebaseAuthHandle,
    provider: GoogleAuthProviderInstance,
  ) => Promise<{ user: FirebaseAuthUser }>
  signOut: (auth: FirebaseAuthHandle) => Promise<void>
}

const defaultFirebaseAuthApi: FirebaseAuthApi = {
  EmailAuthProvider: {
    credential: (email, password) =>
      EmailAuthProvider.credential(email, password),
  },
  GoogleAuthProvider: class extends GoogleAuthProvider {
    static override credentialFromError(error: unknown) {
      return error instanceof FirebaseError
        ? GoogleAuthProvider.credentialFromError(error)
        : null
    }
  },
  createUserWithEmailAndPassword: (auth, email, password) =>
    createUserWithEmailAndPassword(fromFirebaseAuth(auth), email, password),
  getIdToken: (user, forceRefresh) =>
    getIdToken(fromFirebaseUser(user), forceRefresh),
  linkWithCredential: async (user, credential) => {
    await linkWithCredential(
      fromFirebaseUser(user),
      fromFirebaseCredential(credential),
    )
  },
  onIdTokenChanged: (auth, listener) =>
    onIdTokenChanged(fromFirebaseAuth(auth), listener),
  reauthenticateWithCredential: async (user, credential) => {
    await reauthenticateWithCredential(
      fromFirebaseUser(user),
      fromFirebaseCredential(credential),
    )
  },
  reload: (user) => reload(fromFirebaseUser(user)),
  sendEmailVerification: (user, settings) =>
    sendEmailVerification(fromFirebaseUser(user), settings),
  sendPasswordResetEmail: (auth, email, settings) =>
    sendPasswordResetEmail(fromFirebaseAuth(auth), email, settings),
  signInWithEmailAndPassword: (auth, email, password) =>
    signInWithEmailAndPassword(fromFirebaseAuth(auth), email, password),
  signInWithPopup: (auth, provider) =>
    signInWithPopup(fromFirebaseAuth(auth), fromGoogleAuthProvider(provider)),
  signOut: (auth) => firebaseSignOut(fromFirebaseAuth(auth)),
}

function fromFirebaseAuth(auth: FirebaseAuthHandle): Auth {
  if (!("currentUser" in auth)) {
    throw new TypeError("invalid Firebase auth handle")
  }
  // SAFETY: production always passes the Firebase Auth instance through this seam.
  return auth as Auth
}

function fromFirebaseUser(user: FirebaseAuthUser): User {
  if (!("uid" in user)) {
    throw new TypeError("invalid Firebase user")
  }
  // SAFETY: production always passes the Firebase User through this seam.
  return user as User
}

function fromFirebaseCredential(
  credential: FirebaseAuthCredential,
): AuthCredential {
  if (!credential.providerId) {
    throw new TypeError("invalid Firebase credential")
  }
  // SAFETY: production always passes the Firebase credential through this seam.
  return credential as AuthCredential
}

function fromGoogleAuthProvider(provider: GoogleAuthProviderInstance) {
  // SAFETY: production always passes a GoogleAuthProvider instance.
  return provider as GoogleAuthProvider
}

export type FirebaseAuthContextValue = {
  cancelProviderLink: () => Promise<void>
  fetchAccessToken: FetchAccessToken
  identity: FirebaseIdentity
  providerLink: ProviderLinkState
  reauthenticate: (password: string) => Promise<void>
  refreshIdentity: () => Promise<IdentityJourneyResult>
  requestPasswordReset: (email: string) => Promise<void>
  sendVerification: () => Promise<IdentityJourneyResult>
  signInWithGoogle: () => Promise<IdentityJourneyResult>
  signInWithPassword: (
    email: string,
    password: string,
  ) => Promise<IdentityJourneyResult>
  signOut: () => Promise<void>
  signUpWithPassword: (
    email: string,
    password: string,
  ) => Promise<IdentityJourneyResult>
}

type PendingProviderLink = {
  credential: FirebaseAuthCredential
  email: string
}

const FirebaseAuthContext = createContext<FirebaseAuthContextValue | null>(null)

const unavailableJourney = async (): Promise<IdentityJourneyResult> => ({
  kind: "unavailable",
})

const fallbackAuth: FirebaseAuthContextValue = {
  cancelProviderLink: async () => {},
  fetchAccessToken: async () => null,
  identity: { kind: "signedOut" },
  providerLink: { kind: "none" },
  reauthenticate: async () => {},
  refreshIdentity: unavailableJourney,
  requestPasswordReset: async () => {},
  sendVerification: unavailableJourney,
  signInWithGoogle: unavailableJourney,
  signInWithPassword: unavailableJourney,
  signOut: async () => {},
  signUpWithPassword: unavailableJourney,
}

export function TestFirebaseAuthProvider({
  children,
  value,
}: PropsWithChildren<{ value: Partial<FirebaseAuthContextValue> }>) {
  return (
    <FirebaseAuthContext.Provider value={{ ...fallbackAuth, ...value }}>
      {children}
    </FirebaseAuthContext.Provider>
  )
}

export function FirebaseAuthProvider({
  auth,
  authApi = defaultFirebaseAuthApi,
  children,
}: PropsWithChildren<{ auth: FirebaseAuthHandle; authApi?: FirebaseAuthApi }>) {
  const [identity, setIdentity] = useState<FirebaseIdentity>({
    kind: "loading",
  })
  const [pendingLink, setPendingLink] = useState<PendingProviderLink | null>(
    null,
  )

  useEffect(
    () =>
      authApi.onIdTokenChanged(auth, (user) => {
        setIdentity((current) => settledIdentity(current, user))
      }),
    [auth, authApi],
  )

  const fetchAccessToken = useCallback<FetchAccessToken>(
    async ({ forceRefreshToken }) => {
      const current = auth.currentUser
      return current ? authApi.getIdToken(current, forceRefreshToken) : null
    },
    [auth, authApi],
  )
  const signInWithPassword = useCallback<
    FirebaseAuthContextValue["signInWithPassword"]
  >(
    async (email: string, password: string) => {
      try {
        const result = await authApi.signInWithEmailAndPassword(
          auth,
          email,
          password,
        )
        return await finishProviderLink({
          auth,
          authApi,
          pendingLink,
          setPendingLink,
          user: result.user,
        })
      } catch (error) {
        return journeyFailure(error)
      }
    },
    [auth, authApi, pendingLink],
  )
  const signUpWithPassword = useCallback<
    FirebaseAuthContextValue["signUpWithPassword"]
  >(
    async (email: string, password: string) => {
      try {
        const result = await authApi.createUserWithEmailAndPassword(
          auth,
          email,
          password,
        )
        await authApi.sendEmailVerification(result.user, emailActionSettings())
        return completeResult()
      } catch (error) {
        if (isFirebaseError(error, "auth/email-already-in-use")) {
          setPendingLink({
            credential: authApi.EmailAuthProvider.credential(email, password),
            email: normalizedIdentityEmail(email),
          })
          return { kind: "linkRequired" }
        }
        return journeyFailure(error)
      }
    },
    [auth, authApi],
  )
  const signInWithGoogle = useCallback<
    FirebaseAuthContextValue["signInWithGoogle"]
  >(async () => {
    const provider = new authApi.GoogleAuthProvider()
    provider.setCustomParameters({ prompt: "select_account" })
    try {
      const result = await authApi.signInWithPopup(auth, provider)
      return await finishProviderLink({
        auth,
        authApi,
        pendingLink,
        setPendingLink,
        user: result.user,
      })
    } catch (error) {
      const collision = googleProviderCollision(error, authApi)
      if (collision) {
        setPendingLink(collision)
        return { kind: "linkRequired" }
      }
      return journeyFailure(error)
    }
  }, [auth, authApi, pendingLink])
  const requestPasswordReset = useCallback(
    async (email: string) => {
      try {
        await authApi.sendPasswordResetEmail(auth, email, emailActionSettings())
      } catch {
        // Firebase outcomes must remain indistinguishable to prevent enumeration.
      }
    },
    [auth, authApi],
  )
  const sendVerification = useCallback<
    FirebaseAuthContextValue["sendVerification"]
  >(async () => {
    const current = auth.currentUser
    if (!current) return { kind: "unavailable" }
    try {
      await authApi.sendEmailVerification(current, emailActionSettings())
      return completeResult()
    } catch (error) {
      return journeyFailure(error)
    }
  }, [auth, authApi])
  const refreshIdentity = useCallback<
    FirebaseAuthContextValue["refreshIdentity"]
  >(async () => {
    const current = auth.currentUser
    if (!current) return { kind: "unavailable" }
    try {
      await authApi.reload(current)
      await authApi.getIdToken(current, true)
      setIdentity((identity) => settledIdentity(identity, auth.currentUser))
      return completeResult()
    } catch (error) {
      return journeyFailure(error)
    }
  }, [auth, authApi])
  const reauthenticate = useCallback(
    async (password: string) => {
      const current = auth.currentUser
      if (!current?.email) {
        throw new Error("Email/password reauthentication is unavailable")
      }
      await authApi.reauthenticateWithCredential(
        current,
        authApi.EmailAuthProvider.credential(current.email, password),
      )
    },
    [auth, authApi],
  )
  const cancelProviderLink = useCallback(async () => {
    setPendingLink(null)
    if (auth.currentUser) await authApi.signOut(auth)
  }, [auth, authApi])
  const signOut = useCallback(async () => {
    setPendingLink(null)
    // Reviewed Games are Player data on a possibly shared device, so the
    // cached bytes go before the credential does. A purge that fails still
    // signs out, and the cache is keyed by uid, so the next Player cannot
    // read what is left.
    await purgeReviewSnapshotCache()
    await authApi.signOut(auth)
  }, [auth, authApi])
  const providerLink = useMemo<ProviderLinkState>(
    () =>
      pendingLink
        ? { kind: "required", email: pendingLink.email }
        : { kind: "none" },
    [pendingLink],
  )
  const value = useMemo<FirebaseAuthContextValue>(
    () => ({
      cancelProviderLink,
      fetchAccessToken,
      identity,
      providerLink,
      reauthenticate,
      refreshIdentity,
      requestPasswordReset,
      sendVerification,
      signInWithGoogle,
      signInWithPassword,
      signOut,
      signUpWithPassword,
    }),
    [
      cancelProviderLink,
      fetchAccessToken,
      identity,
      providerLink,
      reauthenticate,
      refreshIdentity,
      requestPasswordReset,
      sendVerification,
      signInWithGoogle,
      signInWithPassword,
      signOut,
      signUpWithPassword,
    ],
  )

  return (
    <FirebaseAuthContext.Provider value={value}>
      {children}
    </FirebaseAuthContext.Provider>
  )
}

export function useFirebaseAuth() {
  const context = useContext(FirebaseAuthContext)
  if (!context) {
    throw new Error("FirebaseAuthProvider is required")
  }
  return context
}

function projectIdentity(user: FirebaseAuthUser | null): FirebaseIdentity {
  return user
    ? {
        kind: "signedIn",
        email: user.email,
        emailVerified: user.emailVerified,
        playerId: user.uid,
      }
    : { kind: "signedOut" }
}

/**
 * The identity to keep once Firebase reports a user, reusing the settled one
 * when the Player did not actually change.
 *
 * Every forced token refresh notifies the id-token listener, and a fresh object
 * for the same Player reads downstream as a different identity: the Beta Access
 * check restarts, the authorized page it gates unmounts, and remounting starts
 * the next forced refresh. Handing back the settled object is what stops that
 * from repeating for as long as the page is open.
 */
function settledIdentity(
  settled: FirebaseIdentity,
  user: FirebaseAuthUser | null,
): FirebaseIdentity {
  const projected = projectIdentity(user)
  return sameIdentity(settled, projected) ? settled : projected
}

function sameIdentity(left: FirebaseIdentity, right: FirebaseIdentity) {
  if (left.kind !== "signedIn" || right.kind !== "signedIn") {
    return left.kind === right.kind
  }
  return (
    left.email === right.email &&
    left.emailVerified === right.emailVerified &&
    left.playerId === right.playerId
  )
}

async function finishProviderLink({
  auth,
  authApi,
  pendingLink,
  setPendingLink,
  user,
}: {
  auth: FirebaseAuthHandle
  authApi: FirebaseAuthApi
  pendingLink: PendingProviderLink | null
  setPendingLink: (pending: PendingProviderLink | null) => void
  user: FirebaseAuthUser
}): Promise<IdentityJourneyResult> {
  if (!pendingLink) return completeResult()
  if (
    !user.email ||
    normalizedIdentityEmail(user.email) !== pendingLink.email
  ) {
    await authApi.signOut(auth)
    return { kind: "wrongAccount" }
  }
  if (
    user.providerData.some(
      ({ providerId }) => providerId === pendingLink.credential.providerId,
    )
  ) {
    setPendingLink(null)
    return completeResult()
  }
  try {
    await authApi.linkWithCredential(user, pendingLink.credential)
    setPendingLink(null)
    return completeResult()
  } catch (error) {
    return journeyFailure(error)
  }
}

function googleProviderCollision(
  error: unknown,
  authApi: FirebaseAuthApi,
): PendingProviderLink | null {
  if (
    !isFirebaseError(error, "auth/account-exists-with-different-credential")
  ) {
    return null
  }
  const email = authErrorEmail(error)
  const credential = authApi.GoogleAuthProvider.credentialFromError(error)
  return email && credential
    ? { credential, email: normalizedIdentityEmail(email) }
    : null
}

function authErrorEmail(error: FirebaseError): string | null {
  const parsed = v.safeParse(
    v.pipe(v.string(), v.minLength(1)),
    error.customData?.email,
  )
  return parsed.success && parsed.output.trim() ? parsed.output : null
}

function normalizedIdentityEmail(email: string) {
  return email.trim().toLowerCase()
}

function emailActionSettings(): ActionCodeSettings {
  return {
    url: new URL("/login/", window.location.origin).href,
  }
}

function journeyFailure(error: unknown): IdentityJourneyResult {
  if (
    isFirebaseError(error, "auth/popup-closed-by-user") ||
    isFirebaseError(error, "auth/cancelled-popup-request") ||
    isFirebaseError(error, "auth/redirect-cancelled-by-user")
  ) {
    return { kind: "canceled" }
  }
  if (
    isFirebaseError(error, "auth/invalid-credential") ||
    isFirebaseError(error, "auth/invalid-email") ||
    isFirebaseError(error, "auth/user-mismatch") ||
    isFirebaseError(error, "auth/wrong-password")
  ) {
    return { kind: "invalidCredentials" }
  }
  if (isFirebaseError(error, "auth/too-many-requests")) {
    return { kind: "tryLater" }
  }
  return { kind: "unavailable" }
}

function isFirebaseError(error: unknown, code: string): error is FirebaseError {
  return error instanceof FirebaseError && error.code === code
}

function completeResult(): IdentityJourneyResult {
  return { kind: "complete" }
}
