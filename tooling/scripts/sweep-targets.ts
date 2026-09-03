/**
 * Reclaim Cargo target space across this checkout and its sibling Jujutsu
 * workspaces and Git worktrees.
 *
 * mbx bounds its own action store, but the development shells deliberately
 * leave `CARGO_TARGET_DIR` unset and set `MBX_TARGET_VIEWS=0`, so every
 * workspace keeps an unbounded target directory that mbx does not manage.
 * `mbx gc` cannot own this cleanup: it collects the action store and the
 * managed target directories mbx placed, and this repository places none.
 * Cleanup therefore stays explicitly scoped to roots discovered from this
 * repository rather than a recursive scan of a home directory.
 *
 * Invoke once after Rust implementation and its focused checks, never before
 * compiling and never for non-Rust work. The command rate-limits itself: each
 * target directory carries a stamp recording its last sweep, and a root is
 * skipped until the interval has elapsed. A start-of-task sweep would discard
 * incremental state the upcoming compile still needs; a due post-task run
 * removes untouched leftovers without deleting today's artifacts.
 */

import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";

import { readJsonObject } from "@chenchess/coach-engine-sdk";
const DEFAULT_DAYS = 3;
const DEFAULT_INTERVAL_DAYS = 7;
const STAMP_NAME = ".chenchess-sweep-stamp.json";
const MILLISECONDS_PER_DAY = 24 * 60 * 60 * 1000;

export type SweepOptions = {
  days: number;
  dryRun: boolean;
  force: boolean;
  intervalDays: number;
  maxSize: string | null;
  purgeIncremental: boolean;
};

export type SweepStamp = {
  sweptAt: string;
  days: number;
};

export type SweepPlan = {
  roots: string[];
  options: SweepOptions;
};

export class SweepFailure extends Error {
  constructor(message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = "SweepFailure";
  }
}

export function parseOptions(argv: readonly string[]): SweepOptions {
  const options: SweepOptions = {
    days: DEFAULT_DAYS,
    dryRun: false,
    force: false,
    intervalDays: DEFAULT_INTERVAL_DAYS,
    maxSize: null,
    purgeIncremental: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--dry-run") {
      options.dryRun = true;
    } else if (argument === "--force") {
      options.force = true;
    } else if (argument === "--purge-incremental") {
      options.purgeIncremental = true;
    } else if (argument === "--days") {
      options.days = requireDays(argv[index + 1]);
      index += 1;
    } else if (argument?.startsWith("--days=")) {
      options.days = requireDays(argument.slice("--days=".length));
    } else if (argument === "--interval-days") {
      options.intervalDays = requireDays(argv[index + 1], "--interval-days");
      index += 1;
    } else if (argument?.startsWith("--interval-days=")) {
      options.intervalDays = requireDays(
        argument.slice("--interval-days=".length),
        "--interval-days",
      );
    } else if (argument === "--max-size") {
      options.maxSize = requireMaxSize(argv[index + 1]);
      index += 1;
    } else if (argument?.startsWith("--max-size=")) {
      options.maxSize = requireMaxSize(argument.slice("--max-size=".length));
    } else {
      throw new SweepFailure(`unknown argument: ${argument}`);
    }
  }

  return options;
}

function requireDays(value: string | undefined, flag = "--days"): number {
  const days = Number(value);
  if (!Number.isInteger(days) || days < 0) {
    throw new SweepFailure(`${flag} requires a non-negative whole number`);
  }
  return days;
}

function requireMaxSize(value: string | undefined): string {
  if (value === undefined || !/^\d+(?:[KMG]i?B)?$/.test(value)) {
    throw new SweepFailure(
      "--max-size requires a size such as 20GB; the unit defaults to MB",
    );
  }
  return value;
}

/** Extract the checkout paths from `git worktree list --porcelain` output. */
export function parseGitWorktreePaths(porcelain: string): string[] {
  return porcelain
    .split("\n")
    .filter((line) => line.startsWith("worktree "))
    .map((line) => line.slice("worktree ".length).trim())
    .filter((path) => path.length > 0);
}

/**
 * A secondary Jujutsu workspace stores `.jj/repo` as a file naming the backing
 * repository directory. Only workspaces backed by this repository are in scope.
 */
export function isLinkedJjWorkspace(
  repoPointer: string,
  repoRoot: string,
): boolean {
  return resolve(repoPointer.trim()) === resolve(repoRoot, ".jj/repo");
}

/**
 * Keep only candidates that are this checkout or one of its siblings and that
 * actually contain a Cargo project. An unrecognized path is dropped rather
 * than swept, so a stale worktree registration can never widen the scope.
 */
export function selectSweepRoots(
  repoRoot: string,
  candidates: readonly string[],
  isCargoProject: (path: string) => boolean,
): string[] {
  const root = resolve(repoRoot);
  const parent = dirname(root);
  const selected = new Set<string>();

  for (const candidate of candidates) {
    const path = resolve(candidate);
    if (path !== root && dirname(path) !== parent) continue;
    if (!isCargoProject(path)) continue;
    selected.add(path);
  }

  return [...selected].sort();
}

export function sweepArguments(plan: SweepPlan): string[] {
  const { options } = plan;
  const argv = ["sweep", "--time", String(options.days)];
  if (options.maxSize !== null) argv.push("--maxsize", options.maxSize);
  if (options.dryRun) argv.push("--dry-run");
  return [...argv, ...plan.roots];
}

const UNIT_BYTES = {
  B: 1,
  KiB: 1024,
  MiB: 1024 ** 2,
  GiB: 1024 ** 3,
  TiB: 1024 ** 4,
};

function unitBytes(unit: string): number | undefined {
  switch (unit) {
    case "B":
    case "KiB":
    case "MiB":
    case "GiB":
    case "TiB":
      return UNIT_BYTES[unit];
    default:
      return undefined;
  }
}

/**
 * cargo-sweep reports one human-readable total per root, as `Cleaned` or, under
 * a dry run, `Would clean:`. Absence of any total is not a failure. The total
 * is logical bytes: APFS clones and hardlinks are counted once per path, so the
 * physical space a sweep returns is usually smaller than the reported figure.
 */
export function parseCleanedBytes(output: string): number | null {
  const pattern =
    /(?:Cleaned|Would clean:)\s+([\d.]+)\s+(B|KiB|MiB|GiB|TiB)\b/g;
  let total: number | null = null;

  for (const match of output.matchAll(pattern)) {
    const amount = Number(match[1]);
    const unit = unitBytes(match[2] ?? "");
    if (!Number.isFinite(amount) || unit === undefined) continue;
    total = (total ?? 0) + amount * unit;
  }

  return total;
}

export function formatBytes(bytes: number): string {
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

/** Every root records its own last sweep beside the artifacts it describes. */
export function stampPath(root: string): string {
  return join(root, "target", STAMP_NAME);
}

/**
 * An unreadable, malformed, or future-dated stamp is treated as absent, so a
 * corrupted file makes the root due rather than permanently skipped.
 */
export function parseStamp(contents: string): SweepStamp | null {
  try {
    const parsed = readJsonObject(JSON.parse(contents) as unknown);
    if (parsed === undefined) return null;
    const sweptAt = parsed.sweptAt;
    const days = parsed.days;
    if (typeof sweptAt !== "string" || typeof days !== "number") return null;
    if (Number.isNaN(Date.parse(sweptAt))) return null;
    return { sweptAt, days };
  } catch {
    return null;
  }
}

export function readStamp(root: string): SweepStamp | null {
  const path = stampPath(root);
  if (!existsSync(path)) return null;
  try {
    return parseStamp(readFileSync(path, "utf8"));
  } catch {
    return null;
  }
}

export function writeStamp(root: string, stamp: SweepStamp): void {
  const path = stampPath(root);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, `${JSON.stringify(stamp, null, 2)}\n`);
}

/**
 * A root is due once the interval has elapsed since its last sweep. A stamp
 * dated in the future is not trusted, and a stamp recording a longer retention
 * than the current request cannot satisfy it, so both make the root due.
 */
export function isSweepDue(
  stamp: SweepStamp | null,
  now: Date,
  options: Pick<SweepOptions, "days" | "intervalDays" | "force">,
): boolean {
  if (options.force || stamp === null) return true;
  if (stamp.days > options.days) return true;

  const elapsed = now.getTime() - Date.parse(stamp.sweptAt);
  if (elapsed < 0) return true;
  return elapsed >= options.intervalDays * MILLISECONDS_PER_DAY;
}

export function nextSweepDue(stamp: SweepStamp, intervalDays: number): Date {
  return new Date(
    Date.parse(stamp.sweptAt) + intervalDays * MILLISECONDS_PER_DAY,
  );
}

export function discoverRoots(repoRoot: string): string[] {
  const candidates = new Set<string>([repoRoot]);

  for (const path of gitWorktreePaths(repoRoot)) candidates.add(path);
  for (const path of siblingJjWorkspaces(repoRoot)) candidates.add(path);

  // A workspace that has never been built has nothing to reclaim, and passing
  // it to cargo-sweep only produces a missing-target warning.
  return selectSweepRoots(
    repoRoot,
    [...candidates],
    (path) =>
      existsSync(join(path, "Cargo.toml")) && existsSync(join(path, "target")),
  );
}

function gitWorktreePaths(repoRoot: string): string[] {
  const result = Bun.spawnSync({
    cmd: ["git", "worktree", "list", "--porcelain"],
    cwd: repoRoot,
    stdout: "pipe",
    stderr: "pipe",
  });
  if (result.exitCode !== 0) return [];
  return parseGitWorktreePaths(result.stdout.toString());
}

function siblingJjWorkspaces(repoRoot: string): string[] {
  const parent = dirname(resolve(repoRoot));
  const workspaces: string[] = [];

  for (const entry of readdirSync(parent, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const pointer = join(parent, entry.name, ".jj/repo");
    if (!existsSync(pointer)) continue;
    try {
      if (isLinkedJjWorkspace(readFileSync(pointer, "utf8"), repoRoot)) {
        workspaces.push(realpathSync(join(parent, entry.name)));
      }
    } catch {
      // A directory whose pointer cannot be read stays out of scope.
    }
  }

  return workspaces;
}

/**
 * mbx restores compiled artifacts but bypasses incremental state, so an
 * incremental directory is the one thing a sweep cannot refill cheaply.
 * Purging it is therefore opt-in.
 */
export function incrementalDirectories(root: string): string[] {
  const target = join(root, "target");
  if (!existsSync(target)) return [];

  const directories: string[] = [];
  for (const entry of readdirSync(target, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const path = join(target, entry.name, "incremental");
    if (existsSync(path)) directories.push(path);
  }
  return directories;
}

function purgeIncremental(roots: readonly string[], dryRun: boolean): void {
  for (const root of roots) {
    for (const directory of incrementalDirectories(root)) {
      if (dryRun) {
        process.stdout.write(`would remove ${directory}\n`);
        continue;
      }
      rmSync(directory, { force: true, recursive: true });
      process.stdout.write(`removed ${directory}\n`);
    }
  }
}

function requireCargoSweep(): string {
  const result = Bun.spawnSync({
    cmd: ["cargo-sweep", "--version"],
    stdout: "pipe",
    stderr: "pipe",
  });
  if (result.exitCode !== 0) {
    throw new SweepFailure(
      "cargo-sweep is unavailable; enter the shell with ./tooling/nix-develop or use a Cloud Agent image that bakes cargo-sweep",
    );
  }
  return result.stdout.toString().trim();
}

export function run(repoRoot: string, options: SweepOptions): number {
  const discovered = discoverRoots(repoRoot);
  if (discovered.length === 0) {
    throw new SweepFailure("no Cargo workspace roots were discovered");
  }

  process.stdout.write(`${requireCargoSweep()}\n`);

  const now = new Date();
  const roots: string[] = [];
  for (const root of discovered) {
    const stamp = readStamp(root);
    if (stamp === null || isSweepDue(stamp, now, options)) {
      roots.push(root);
      process.stdout.write(`root ${root} (due)\n`);
      continue;
    }
    const due = nextSweepDue(stamp, options.intervalDays);
    process.stdout.write(
      `root ${root} (skipped, swept ${stamp.sweptAt}, next ${due.toISOString()})\n`,
    );
  }

  if (roots.length === 0) {
    process.stdout.write(
      `every root was swept within the last ${options.intervalDays} days; pass --force to sweep anyway\n`,
    );
    return 0;
  }

  const result = Bun.spawnSync({
    cmd: ["cargo-sweep", ...sweepArguments({ roots, options })],
    cwd: repoRoot,
    stdout: "pipe",
    stderr: "pipe",
  });
  const output = `${result.stdout.toString()}${result.stderr.toString()}`;
  process.stdout.write(output);
  if (result.exitCode !== 0) {
    throw new SweepFailure(`cargo-sweep exited with ${result.exitCode}`);
  }

  // Purge after the sweep so cargo-sweep's own total is not inflated by
  // incremental files this script had already deleted.
  if (options.purgeIncremental) purgeIncremental(roots, options.dryRun);

  const cleaned = parseCleanedBytes(output);
  if (cleaned !== null) {
    process.stdout.write(
      `${options.dryRun ? "would reclaim" : "reclaimed"} ${formatBytes(cleaned)} of Cargo artifacts\n`,
    );
  }

  // A dry run must not consume the interval it only simulated.
  if (!options.dryRun) {
    const sweptAt = now.toISOString();
    for (const root of roots) writeStamp(root, { sweptAt, days: options.days });
  }
  return 0;
}

if (import.meta.main) {
  try {
    const repoRoot = resolve(import.meta.dir, "../..");
    run(repoRoot, parseOptions(process.argv.slice(2)));
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    process.stderr.write(`sweep-targets: ${message}\n`);
    process.exitCode = 1;
  }
}
