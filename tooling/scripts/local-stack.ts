/**
 * The one description of the local stack, shared by `local:up` and
 * `local:seed`.
 *
 * Every value here is a loopback address on this machine. Coach Engine arms
 * its Firebase Auth emulator identity path on that property alone (ADR 0060),
 * so nothing in this file is meaningful in a deployed environment and Railway
 * sets none of it.
 */
import { createHash, generateKeyPairSync, randomBytes } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

import * as v from "valibot";

export const PROJECT_ID = "chenchess-local";
export const FIRESTORE_DATABASE_ID = "coach-app-staging";

export const AUTH_EMULATOR_HOST = "127.0.0.1:9099";
export const FIRESTORE_EMULATOR_HOST = "127.0.0.1:8081";
export const AUTH_EMULATOR_URL = `http://${AUTH_EMULATOR_HOST}`;
export const FIRESTORE_EMULATOR_URL = `http://${FIRESTORE_EMULATOR_HOST}`;

export const MAIA_BASE_URL = "http://127.0.0.1:8080";
export const COACH_ENGINE_BASE_URL = "http://127.0.0.1:8787";
export const CENTRAL_HOST_URL = "http://127.0.0.1:4173";

/** The Player `local:seed` provisions, and the browser signs in as. */
export const LOCAL_PLAYER = {
  email: "player@chenchess.local",
  password: "local-development-password",
};

export type EmulatorIdentity = v.InferOutput<typeof signInSchema>;

const signInSchema = v.object({
  idToken: v.pipe(v.string(), v.nonEmpty()),
  localId: v.pipe(v.string(), v.nonEmpty()),
});

type AccountQuery = v.InferOutput<typeof accountQuerySchema>;

const accountQuerySchema = v.object({
  userInfo: v.optional(
    v.array(
      v.object({
        email: v.optional(v.string()),
        localId: v.pipe(v.string(), v.nonEmpty()),
      }),
    ),
  ),
});

/**
 * The emulator's admin account surface: an empty body queries, and a body
 * with an address creates or, with a `localId`, updates.
 */
type AccountRequest =
  | Record<string, never>
  | {
      email: string;
      emailVerified: boolean;
      localId?: string;
      password: string;
    };

/**
 * Firebase's own emulator admin surface authorizes with this literal, not with
 * a credential. It is the documented value and carries no secret.
 */
const ADMIN_AUTHORIZATION = "Bearer owner";

const accountsUrl = `${AUTH_EMULATOR_URL}/identitytoolkit.googleapis.com/v1/projects/${PROJECT_ID}/accounts`;

export class LocalStackFailure extends Error {
  constructor(message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = "LocalStackFailure";
  }
}

/**
 * Creates or reasserts the Player's emulator account and signs in, returning a
 * live ID token. Idempotent: a second run reasserts `emailVerified` rather
 * than trusting the previous one, because a Player whose address is
 * unverified is redirected away from the dashboard.
 */
export async function provisionLocalPlayer(): Promise<EmulatorIdentity> {
  const existing = (await queryAccounts()).userInfo?.find(
    (user) => user.email === LOCAL_PLAYER.email,
  );
  const account = {
    email: LOCAL_PLAYER.email,
    emailVerified: true,
    password: LOCAL_PLAYER.password,
  };
  await (existing
    ? writeAccount(`${accountsUrl}:update`, {
        ...account,
        localId: existing.localId,
      })
    : writeAccount(accountsUrl, account));
  return signIn();
}

/** Mints a fresh ID token for the seeded Player. */
export async function signIn(): Promise<EmulatorIdentity> {
  const response = await fetch(
    `${AUTH_EMULATOR_URL}/identitytoolkit.googleapis.com/v1/accounts:signInWithPassword?key=${PROJECT_ID}`,
    {
      body: JSON.stringify({
        email: LOCAL_PLAYER.email,
        password: LOCAL_PLAYER.password,
        returnSecureToken: true,
      }),
      headers: { "Content-Type": "application/json" },
      method: "POST",
    },
  );
  if (!response.ok) {
    throw new LocalStackFailure(
      `Auth emulator refused the local sign-in (${response.status})`,
    );
  }
  return v.parse(signInSchema, await response.json());
}

/**
 * Grants Beta Access to the seeded Player.
 *
 * Creating a Firebase identity is not the same thing as authorizing a Player:
 * `AuthorizedPlayer` calls `beta_access::require_access`, which reads
 * `users/<sha256 of the Player ID>/betaAccess/grant`
 * (`services/coach-engine/src/beta_access/firestore/grant.rs`). Without this
 * document a correctly minted emulator token is accepted and then refused.
 *
 * Written directly rather than through the invitation flow because that flow
 * needs an operator surface and a mail transport the local stack does not run.
 */
export async function grantBetaAccess(playerId: string): Promise<void> {
  const document = `users/${sha256Hex(playerId)}/betaAccess/grant`;
  const url = `${FIRESTORE_EMULATOR_URL}/v1/projects/${PROJECT_ID}/databases/${FIRESTORE_DATABASE_ID}/documents/${document}`;
  const response = await fetch(url, {
    body: JSON.stringify({
      fields: {
        // The shape BetaAccessGrantDocument deserializes, with
        // `deny_unknown_fields`: no field here is optional or spare.
        grantedAt: { timestampValue: new Date().toISOString() },
        invitationId: { stringValue: localInvitationId(playerId) },
        schemaVersion: { integerValue: "1" },
      },
    }),
    headers: {
      Authorization: ADMIN_AUTHORIZATION,
      "Content-Type": "application/json",
    },
    method: "PATCH",
  });
  if (!response.ok) {
    throw new LocalStackFailure(
      `Firestore emulator refused the Beta Access grant (${response.status})`,
    );
  }
}

/**
 * The 32 lowercase hex characters `opaque_identifier` demands, derived from
 * the Player so a re-seed rewrites the same grant rather than accumulating
 * distinct ones.
 */
function localInvitationId(playerId: string): string {
  return sha256Hex(`local-invitation:${playerId}`).slice(0, 32);
}

export function sha256Hex(material: string): string {
  return createHash("sha256").update(material).digest("hex");
}

async function queryAccounts(): Promise<AccountQuery> {
  return v.parse(
    accountQuerySchema,
    await adminRequest(`${accountsUrl}:query`, {}),
  );
}

/** Creates or updates one account. Neither answer carries `userInfo`. */
async function writeAccount(url: string, body: AccountRequest): Promise<void> {
  await adminRequest(url, body);
}

async function adminRequest(url: string, body: AccountRequest) {
  const response = await fetch(url, {
    body: JSON.stringify(body),
    headers: {
      Authorization: ADMIN_AUTHORIZATION,
      "Content-Type": "application/json",
    },
    method: "POST",
  });
  if (!response.ok) {
    throw new LocalStackFailure(
      `Auth emulator responded ${response.status} to ${url}`,
    );
  }
  return response.json();
}

/** Polls one loopback endpoint until it answers, or reports what it waited for. */
export async function waitForHealth(
  name: string,
  url: string,
  timeoutMs: number,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  let lastError = "no response";
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(2_000) });
      if (response.ok) return;
      lastError = `HTTP ${response.status}`;
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new LocalStackFailure(
    `${name} did not become ready at ${url}: ${lastError}`,
  );
}

const oauthKeysSchema = v.object({
  cookieKeys: v.array(v.pipe(v.string(), v.minLength(32))),
  privateJwks: v.pipe(v.string(), v.nonEmpty()),
  publicJwks: v.pipe(v.string(), v.nonEmpty()),
});

export type LocalOAuthKeys = v.InferOutput<typeof oauthKeysSchema>;

/**
 * The OAuth signing key and cookie keys the local stack runs on, generated
 * once and kept in the ignored emulator directory.
 *
 * Generated rather than checked in, because a committed private signing key is
 * a real key whatever a README calls it. Kept rather than regenerated, because
 * a Coach MCP access token issued before a restart must still verify after
 * one, and Coach Engine reads the public half of this same pair.
 */
export function localOAuthKeys(path: string): LocalOAuthKeys {
  const stored = readStoredKeys(path);
  if (stored) return stored;
  const { privateKey, publicKey } = generateKeyPairSync("rsa", {
    modulusLength: 2048,
  });
  const kid = randomBytes(16).toString("hex");
  const signing = { alg: "RS256", kid, use: "sig" };
  const keys = {
    cookieKeys: [
      randomBytes(32).toString("hex"),
      randomBytes(32).toString("hex"),
    ],
    privateJwks: JSON.stringify({
      keys: [{ ...privateKey.export({ format: "jwk" }), ...signing }],
    }),
    publicJwks: JSON.stringify({
      keys: [{ ...publicKey.export({ format: "jwk" }), ...signing }],
    }),
  };
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, `${JSON.stringify(keys, null, 2)}\n`, {
    mode: 0o600,
  });
  return keys;
}

function readStoredKeys(path: string): LocalOAuthKeys | null {
  try {
    return v.parse(oauthKeysSchema, JSON.parse(readFileSync(path, "utf8")));
  } catch {
    return null;
  }
}
