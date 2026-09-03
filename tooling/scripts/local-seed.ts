/**
 * Puts the local stack in the state a Player arrives to: an account they can
 * sign in with, Beta Access granted, and one reviewed Game to open.
 *
 * Safe to run again. The account is reasserted, the grant is rewritten at the
 * same address, and a Game this Player already has is left alone rather than
 * imported a second time.
 *
 * Run it after every `local:up`. The Auth emulator exports its accounts on
 * exit, but the Firestore emulator's export carries only the default database
 * and this stack runs on a named one, so a restart comes back with the Player
 * and without their Games.
 */
import * as v from "valibot";

import { run as importGame } from "./local-smoke";
import {
  CENTRAL_HOST_URL,
  COACH_ENGINE_BASE_URL,
  grantBetaAccess,
  LOCAL_PLAYER,
  LocalStackFailure,
  provisionLocalPlayer,
} from "./local-stack";

const importedGamesSchema = v.object({ games: v.array(v.unknown()) });

async function main(): Promise<void> {
  const identity = await provisionLocalPlayer();
  await grantBetaAccess(identity.localId);
  process.stdout.write(
    `Beta Access granted to ${LOCAL_PLAYER.email} (${identity.localId})\n`,
  );

  const existing = await importedGames(identity.idToken);
  if (existing > 0) {
    process.stdout.write(
      `${existing} Game already reviewed for this Player; not importing again\n`,
    );
  } else {
    process.stdout.write("importing a Game through the real review path\n");
    const review = await importGame(COACH_ENGINE_BASE_URL, identity.idToken);
    process.stdout.write(
      `imported a Game with ${review.review.criticalMoments.length} Critical Moments\n`,
    );
  }

  process.stdout.write(
    [
      "",
      `sign in at ${CENTRAL_HOST_URL}/login`,
      `  email     ${LOCAL_PLAYER.email}`,
      `  password  ${LOCAL_PLAYER.password}`,
      "",
      "to drive the API directly:",
      `  AUTH_TOKEN=${identity.idToken} bun run smoke:local`,
      "",
    ].join("\n"),
  );
}

async function importedGames(authToken: string): Promise<number> {
  const response = await fetch(
    `${COACH_ENGINE_BASE_URL}/api/v1/imported-games`,
    {
      headers: { Authorization: `Bearer ${authToken}` },
      signal: AbortSignal.timeout(30_000),
    },
  );
  if (!response.ok) {
    throw new LocalStackFailure(
      `Coach Engine refused the seeded Player's Imported Games (${response.status})`,
    );
  }
  return v.parse(importedGamesSchema, await response.json()).games.length;
}

main().catch((error: unknown) => {
  process.stderr.write(
    `local seed failed: ${error instanceof Error ? error.message : String(error)}\n`,
  );
  process.exitCode = 1;
});
