/**
 * Brings up the whole product on this machine: a Firebase Authentication
 * clone, a Firestore clone, the Human Move Model, Coach Engine, and Central
 * Host. Nothing is deployed and no Google credential is read.
 *
 * Run `bun run local:seed` in a second terminal once this reports ready. That
 * is what provisions a Player and grants Beta Access; an account on its own is
 * accepted by Coach Engine and then refused.
 */
import { existsSync, mkdirSync } from "node:fs";
import { resolve } from "node:path";

import {
  AUTH_EMULATOR_HOST,
  AUTH_EMULATOR_URL,
  CENTRAL_HOST_URL,
  COACH_ENGINE_BASE_URL,
  FIRESTORE_DATABASE_ID,
  FIRESTORE_EMULATOR_HOST,
  FIRESTORE_EMULATOR_URL,
  LocalStackFailure,
  MAIA_BASE_URL,
  PROJECT_ID,
  waitForHealth,
} from "./local-stack";

const REPOSITORY_ROOT = resolve(import.meta.dirname, "../..");
const EMULATOR_DATA = resolve(REPOSITORY_ROOT, ".local-emulator/data");

/**
 * The browser identity the Auth emulator answers for. None of it is a
 * credential: the emulator accepts any API key, and it is the emulator URL
 * below that decides where sign-in goes.
 */
const FIREBASE_WEB_CONFIG = {
  apiKey: "local-emulator-key",
  appId: "1:0:web:local",
  authDomain: `${PROJECT_ID}.firebaseapp.com`,
  projectId: PROJECT_ID,
};

/**
 * Beta Access hashes its rate-limit keys with this. It protects request
 * volume, not a Player, and a local stack has one Player and no request
 * volume, so a fixed literal is the honest value rather than a generated one
 * a developer would have to carry between runs.
 */
const LOCAL_RATE_LIMIT_KEY = "chenchess-local-emulator-rate-limit-key";

const MAIA_CONTAINER = "chenchess-maia";
const MAIA_IMAGE = "chenchess-maia:local";
const MAIA_MODEL_VOLUME = "chenchess-maia-models";

type Child = { name: string; process: Bun.Subprocess };

const children: Child[] = [];
let shuttingDown = false;

async function main(): Promise<void> {
  process.once("SIGINT", shutdown);
  process.once("SIGTERM", shutdown);

  await refuseOccupiedPorts();
  startEmulators();
  await waitForHealth(
    "the Firebase Auth emulator",
    `${AUTH_EMULATOR_URL}/emulator/v1/projects/${PROJECT_ID}/config`,
    120_000,
  );
  await waitForHealth(
    "the Firestore emulator",
    `${FIRESTORE_EMULATOR_URL}/`,
    120_000,
  );

  await startMaia();
  await waitForHealth(
    "the Human Move Model",
    `${MAIA_BASE_URL}/health`,
    600_000,
  );

  await startCoachEngine();
  await waitForHealth(
    "Coach Engine",
    `${COACH_ENGINE_BASE_URL}/health`,
    180_000,
  );

  startCentralHost();
  await waitForHealth("Central Host", CENTRAL_HOST_URL, 180_000);

  report();
  await Promise.all(children.map((child) => child.process.exited));
}

function startEmulators(): void {
  mkdirSync(EMULATOR_DATA, { recursive: true });
  // `--import` refuses a directory with no export in it, which is what a first
  // run has. The export written on exit is what makes the second run restore.
  const importsExisting = existsSync(
    resolve(EMULATOR_DATA, "firebase-export-metadata.json"),
  );
  spawn("emulators", resolve(REPOSITORY_ROOT, "node_modules/.bin/firebase"), [
    "emulators:start",
    "--only",
    "auth,firestore",
    "--project",
    PROJECT_ID,
    "--config",
    "firebase.local.json",
    ...(importsExisting ? ["--import", EMULATOR_DATA] : []),
    "--export-on-exit",
    EMULATOR_DATA,
  ]);
}

/**
 * The Human Move Model is the one process that stays in a container. Its
 * `maia2` and PyTorch dependencies are not in the development shell, and its
 * model files are hundreds of megabytes that belong in a volume rather than in
 * a checkout. It holds no local state, so it is left running between stacks.
 */
async function startMaia(): Promise<void> {
  if (await reachable(`${MAIA_BASE_URL}/health`)) return;
  if (!(await runToCompletion("docker", ["image", "inspect", MAIA_IMAGE]))) {
    process.stdout.write("building the Human Move Model image, once\n");
    const built = await runToCompletion("docker", [
      "build",
      "--file",
      "services/maia/Dockerfile",
      "--tag",
      MAIA_IMAGE,
      ".",
    ]);
    if (!built)
      throw new LocalStackFailure("the Human Move Model image did not build");
  }
  await runToCompletion("docker", ["rm", "--force", MAIA_CONTAINER]);
  const started = await runToCompletion("docker", [
    "run",
    "--detach",
    "--name",
    MAIA_CONTAINER,
    "--publish",
    "127.0.0.1:8080:8080",
    "--volume",
    `${MAIA_MODEL_VOLUME}:/models`,
    "--env",
    "MAIA_DEVICE=cpu",
    "--env",
    "MAIA_MODEL_DIR=/models",
    "--env",
    "MAIA_MODEL_TYPE=rapid",
    "--env",
    "MAIA_PORT=8080",
    MAIA_IMAGE,
  ]);
  if (!started)
    throw new LocalStackFailure("the Human Move Model container did not start");
  process.stdout.write(
    "the Human Move Model is downloading its model files; the first run takes minutes\n",
  );
}

/**
 * Built first and then run as the built binary, rather than through
 * `cargo run`: Cargo does not forward this process's SIGINT to the engine it
 * spawned, so a `cargo run` child outlives the stack and the next `local:up`
 * fails to bind. Building first also keeps the health wait honest — nothing is
 * polled until the process that answers it exists.
 */
async function startCoachEngine(): Promise<void> {
  process.stdout.write("building Coach Engine\n");
  const built = await runToCompletion(
    resolve(REPOSITORY_ROOT, "tooling/cargo-cached"),
    [
      "build",
      "-p",
      "chen-chess-coach-engine",
      "--bin",
      "chen-chess-coach-engine",
    ],
    "inherit",
  );
  if (!built) throw new LocalStackFailure("Coach Engine did not build");
  spawn(
    "coach-engine",
    resolve(REPOSITORY_ROOT, "target/debug/chen-chess-coach-engine"),
    [],
    {
      BETA_ACCESS_RATE_LIMIT_HMAC_KEY: LOCAL_RATE_LIMIT_KEY,
      DEPLOYMENT_ENVIRONMENT: "staging",
      FIREBASE_AUTH_EMULATOR_HOST: AUTH_EMULATOR_HOST,
      FIREBASE_PROJECT_ID: PROJECT_ID,
      FIRESTORE_DATABASE_ID,
      FIRESTORE_EMULATOR_HOST,
      HOST: "127.0.0.1",
      MAIA_BASE_URL,
      PORT: "8787",
    },
  );
}

function startCentralHost(): void {
  spawn("central-host", "bun", ["run", "--cwd", "apps/central-host", "dev"], {
    COACH_ENGINE_BASE_URL,
    FIREBASE_PROJECT_ID: PROJECT_ID,
    FIREBASE_WEB_CONFIG_JSON: JSON.stringify(FIREBASE_WEB_CONFIG),
    FIRESTORE_EMULATOR_HOST,
    PORT: "4173",
    PUBLIC_URL: CENTRAL_HOST_URL,
    VITE_FIREBASE_API_KEY: FIREBASE_WEB_CONFIG.apiKey,
    VITE_FIREBASE_APP_ID: FIREBASE_WEB_CONFIG.appId,
    VITE_FIREBASE_AUTH_DOMAIN: FIREBASE_WEB_CONFIG.authDomain,
    VITE_FIREBASE_AUTH_EMULATOR_URL: AUTH_EMULATOR_URL,
    VITE_FIREBASE_PROJECT_ID: FIREBASE_WEB_CONFIG.projectId,
  });
}

function report(): void {
  process.stdout.write(
    [
      "",
      "the local stack is up:",
      `  Central Host        ${CENTRAL_HOST_URL}`,
      `  Coach Engine        ${COACH_ENGINE_BASE_URL}`,
      `  Human Move Model    ${MAIA_BASE_URL}`,
      `  Auth emulator       ${AUTH_EMULATOR_URL}`,
      `  Firestore emulator  ${FIRESTORE_EMULATOR_URL} (${FIRESTORE_DATABASE_ID})`,
      "",
      "next, in another terminal: bun run local:seed",
      "",
      "Run the seed after every start. The Auth emulator restores its accounts",
      "from the export; the Firestore emulator's export carries only the",
      "default database, so a restart comes back without the Player's Games.",
      "",
    ].join("\n"),
  );
}

function spawn(
  name: string,
  command: string,
  args: string[],
  environment: Record<string, string> = {},
): void {
  const child = Bun.spawn([command, ...args], {
    cwd: REPOSITORY_ROOT,
    env: { ...process.env, ...environment },
    onExit(_subprocess, exitCode) {
      if (shuttingDown) return;
      process.stderr.write(`${name} exited with ${exitCode ?? "a signal"}\n`);
      shutdown();
    },
    stderr: "inherit",
    stdout: "inherit",
  });
  children.push({ name, process: child });
}

async function runToCompletion(
  command: string,
  args: string[],
  output: "ignore" | "inherit" = "ignore",
): Promise<boolean> {
  const child = Bun.spawn([command, ...args], {
    cwd: REPOSITORY_ROOT,
    stderr: output,
    stdout: output,
  });
  return (await child.exited) === 0;
}

/**
 * A previous stack that did not shut down cleanly presents as a bind failure
 * deep in a service log. Naming the port here is the difference between a
 * fixable message and a confusing one.
 */
async function refuseOccupiedPorts(): Promise<void> {
  const occupied: string[] = [];
  for (const [name, url] of [
    ["Central Host", CENTRAL_HOST_URL],
    ["Coach Engine", `${COACH_ENGINE_BASE_URL}/health`],
    ["the Auth emulator", AUTH_EMULATOR_URL],
    ["the Firestore emulator", FIRESTORE_EMULATOR_URL],
  ] as const) {
    if (await reachable(url)) occupied.push(`${name} (${url})`);
  }
  if (occupied.length > 0) {
    throw new LocalStackFailure(
      `already running, so this stack would not own them: ${occupied.join(", ")}`,
    );
  }
}

async function reachable(url: string): Promise<boolean> {
  try {
    const response = await fetch(url, { signal: AbortSignal.timeout(2_000) });
    return response.ok;
  } catch {
    return false;
  }
}

function shutdown(): void {
  if (shuttingDown) return;
  shuttingDown = true;
  // The emulators write their export on SIGINT, so every child gets the
  // signal it expects rather than a kill.
  for (const child of children) child.process.kill("SIGINT");
  setTimeout(() => process.exit(0), 5_000).unref();
}

main().catch((error: unknown) => {
  process.stderr.write(
    `local stack failed: ${error instanceof Error ? error.message : String(error)}\n`,
  );
  shutdown();
  process.exitCode = 1;
});
