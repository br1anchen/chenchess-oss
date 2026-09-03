import { FirebaseError } from "firebase/app"

import {
  CoachEngineClient,
  mintIdempotencyKey,
  mintOperationId,
  mintRequestId,
  type ReviewSessionCommand,
  type ReviewSessionCommandEnvelope,
  type ReviewSessionEventEnvelope,
} from "@chenchess/coach-engine-sdk"

let localIdentity = 0

export type FetchAccessToken = (options: {
  forceRefreshToken: boolean
}) => Promise<string | null>

export type CommandStreamOptions = {
  envelope: ReviewSessionCommandEnvelope
  fetchAccessToken: FetchAccessToken
  onEvent: (event: ReviewSessionEventEnvelope) => void
}

export type ReviewSessionTransport = {
  createCommandEnvelope: (
    command: ReviewSessionCommand,
  ) => ReviewSessionCommandEnvelope
  streamReviewSessionCommand: (options: CommandStreamOptions) => Promise<void>
}

let providedTransport: ReviewSessionTransport | null = null

export function provideReviewSessionTransport(
  transport: ReviewSessionTransport | null,
): void {
  providedTransport = transport
}

export function createCommandEnvelope(
  command: ReviewSessionCommand,
): ReviewSessionCommandEnvelope {
  if (providedTransport) {
    return providedTransport.createCommandEnvelope(command)
  }
  const identity = nextIdentity()
  return {
    requestId: mintRequestId("web", identity),
    operationId: mintOperationId("web", identity),
    surface: "web",
    command,
  }
}

export function createIdempotencyKey() {
  return mintIdempotencyKey("web", nextIdentity())
}

export async function streamReviewSessionCommand({
  envelope,
  fetchAccessToken,
  onEvent,
}: CommandStreamOptions): Promise<void> {
  if (providedTransport) {
    await providedTransport.streamReviewSessionCommand({
      envelope,
      fetchAccessToken,
      onEvent,
    })
    return
  }
  await new CoachEngineClient({
    credential: () => reviewSessionCredential(fetchAccessToken),
  }).stream(envelope, onEvent)
}

/** Milliseconds between credential retries, tuned for a mobile radio waking
 * up: long enough to matter, short enough to stay under a command's patience. */
const CREDENTIAL_RETRY_DELAYS = [400, 1200]

function parseIsAuthNetworkFailure(caught: unknown): boolean {
  return (
    caught instanceof FirebaseError &&
    caught.code === "auth/network-request-failed"
  )
}

/**
 * The Player's ID token for one Coach Engine command.
 *
 * The cached token is enough — the Firebase SDK refreshes an expiring token
 * on its own — so browsing costs no extra auth round trip. When the fetch
 * still hits the network and that network is flaky (`auth/network-request-
 * failed`), the failure is retried briefly instead of aborting a command
 * that never reached the Coach Engine.
 */
async function reviewSessionCredential(
  fetchAccessToken: FetchAccessToken,
): Promise<string> {
  for (const delayMs of CREDENTIAL_RETRY_DELAYS) {
    try {
      return (await fetchAccessToken({ forceRefreshToken: false })) ?? ""
    } catch (caught) {
      if (!parseIsAuthNetworkFailure(caught)) throw caught
      await delay(delayMs)
    }
  }
  return (await fetchAccessToken({ forceRefreshToken: false })) ?? ""
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds))
}

function nextIdentity(): string {
  localIdentity += 1
  const random =
    globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${localIdentity}`
  return `${random}:${localIdentity}`
}
