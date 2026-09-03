import { useCallback, useEffect, useState } from "react"

import type { FetchAccessToken, FirebaseIdentity } from "./FirebaseAuthProvider"
import {
  checkBetaAuthorization,
  type BetaAuthorization,
} from "./betaAuthorization"

type SignedInIdentity = Extract<FirebaseIdentity, { kind: "signedIn" }>

export type BetaAuthorizationState = BetaAuthorization | { kind: "checking" }

type CompletedAuthorization = {
  playerId: string
  revision: number
  result: BetaAuthorization
}

export type BetaAuthorizationCheck = {
  authorization: BetaAuthorizationState
  refreshAuthorization: () => void
}

const grantedAccessKey = "chenchess.beta-access.granted"

/**
 * UX bridge only: the cached grant skips the "Checking Beta Access" gate while
 * the live check runs. It is client-writable state and is never a substitute
 * for authorization — every Coach Engine call still carries the Firebase token
 * and is authorized server-side; a forged flag renders chrome with no data.
 */
function readGrantedBetaAccess(playerId: string) {
  try {
    return sessionStorage.getItem(grantedAccessKey) === playerId
  } catch {
    return false
  }
}

function writeGrantedBetaAccess(playerId: string | null) {
  try {
    if (playerId) sessionStorage.setItem(grantedAccessKey, playerId)
    else sessionStorage.removeItem(grantedAccessKey)
  } catch {
    // Private mode and storage blocks still have to complete the live check.
  }
}

function sameCompletedAuthorization(
  completed: CompletedAuthorization | null,
  identity: SignedInIdentity,
  revision: number,
) {
  return (
    completed?.playerId === identity.playerId && completed.revision === revision
  )
}

export function useBetaAuthorization(
  fetchAccessToken: FetchAccessToken,
  identity: SignedInIdentity,
): BetaAuthorizationCheck {
  const [completed, setCompleted] = useState<CompletedAuthorization | null>(
    null,
  )
  const [revision, setRevision] = useState(0)
  const refreshAuthorization = useCallback(
    () => setRevision((current) => current + 1),
    [],
  )

  useEffect(() => {
    let active = true
    const checkedPlayerId = identity.playerId
    const checkedRevision = revision
    void checkBetaAuthorization(fetchAccessToken, identity.playerId).then(
      (result) => {
        if (!active) return
        if (result.kind === "granted") writeGrantedBetaAccess(result.playerId)
        else if (result.kind !== "unavailable") writeGrantedBetaAccess(null)
        setCompleted({
          playerId: checkedPlayerId,
          result,
          revision: checkedRevision,
        })
      },
    )
    return () => {
      active = false
    }
  }, [fetchAccessToken, identity, revision])

  if (completed && sameCompletedAuthorization(completed, identity, revision)) {
    return { authorization: completed.result, refreshAuthorization }
  }
  if (readGrantedBetaAccess(identity.playerId)) {
    return {
      authorization: { kind: "granted", playerId: identity.playerId },
      refreshAuthorization,
    }
  }
  return { authorization: { kind: "checking" }, refreshAuthorization }
}
