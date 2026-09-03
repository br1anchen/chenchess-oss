import { afterEach, describe, expect, test } from "bun:test";
import {
  chmod,
  mkdir,
  mkdtemp,
  readFile,
  realpath,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const LAUNCHER = resolve(import.meta.dir, "../nix-develop");
const REPOSITORY_ROOT = resolve(import.meta.dir, "../..");
const FAKE_JJ_REVISION = "1234567890abcdef1234567890abcdef12345678";
const MAINTAINED_NIX_ENTRY_POINTS: string[] = [
  "AGENTS.md",
  "README.md",
  "docs/rust-build-cache.md",
];
const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(
    temporaryDirectories
      .splice(0)
      .map((path) => rm(path, { force: true, recursive: true })),
  );
});

describe("ChenChess Nix launcher", () => {
  test("uses the active Git checkout and forwards the default shell command", async () => {
    const harness = await createHarness();
    const gitRoot = join(harness.root, "git-worktree");
    const nestedWorkingDirectory = join(gitRoot, "apps", "central-host");
    await mkdir(nestedWorkingDirectory, { recursive: true });
    await writeFile(join(gitRoot, "flake.nix"), "{}\n");

    const result = await runLauncher(
      harness,
      nestedWorkingDirectory,
      ["--command", "cargo", "test", "--workspace"],
      {
        FAKE_GIT_MODE: "active",
        FAKE_GIT_ROOT: gitRoot,
      },
    );

    expect(result.exitCode).toBe(0);
    expect(result.stderr).toBe("");
    expect(await capturedArguments(harness)).toEqual([
      "develop",
      gitRoot,
      "--command",
      "cargo",
      "test",
      "--workspace",
    ]);
    expect(await readFile(harness.pwdCapture, "utf8")).toBe(
      nestedWorkingDirectory,
    );
  });

  test("preserves a supported named dev shell", async () => {
    const harness = await createHarness();
    const gitRoot = join(harness.root, "git-worktree");
    await mkdir(gitRoot);
    await writeFile(join(gitRoot, "flake.nix"), "{}\n");

    const result = await runLauncher(
      harness,
      gitRoot,
      [".#vanilla", "--command", "true"],
      {
        FAKE_GIT_MODE: "active",
        FAKE_GIT_ROOT: gitRoot,
      },
    );

    expect(result.exitCode).toBe(0);
    expect(await capturedArguments(harness)).toEqual([
      "develop",
      `${gitRoot}#vanilla`,
      "--command",
      "true",
    ]);
  });

  test("uses the exact Git-backed revision from a native Jujutsu workspace", async () => {
    const harness = await createHarness();
    const workspaceRoot = join(harness.root, "jj-workspace");
    const nestedWorkingDirectory = join(
      workspaceRoot,
      "services",
      "coach-engine",
    );
    const canonicalRoot = join(harness.root, "canonical checkout #1%");
    const encodedCanonicalRoot = `${harness.root}/canonical%20checkout%20%231%25`;
    await Promise.all([
      mkdir(nestedWorkingDirectory, { recursive: true }),
      mkdir(join(canonicalRoot, ".git"), { recursive: true }),
    ]);

    const result = await runLauncher(
      harness,
      nestedWorkingDirectory,
      [".#mbx", "--command", "cargo", "check"],
      {
        FAKE_CANONICAL_ROOT: canonicalRoot,
        FAKE_GIT_MODE: "jj",
        FAKE_JJ_GIT_DIR: join(canonicalRoot, ".git"),
        FAKE_JJ_REVISION,
      },
    );

    expect(result.exitCode).toBe(0);
    expect(result.stderr).toBe("");
    expect(await capturedArguments(harness)).toEqual([
      "develop",
      `git+file://${encodedCanonicalRoot}?rev=${FAKE_JJ_REVISION}#mbx`,
      "--command",
      "cargo",
      "check",
    ]);
    expect(await readFile(harness.pwdCapture, "utf8")).toBe(
      nestedWorkingDirectory,
    );
  });

  test("rejects a failed Jujutsu snapshot before invoking Nix", async () => {
    const harness = await createHarness();
    const workspaceRoot = join(harness.root, "jj-workspace");
    const canonicalRoot = join(harness.root, "canonical");
    await Promise.all([
      mkdir(workspaceRoot),
      mkdir(join(canonicalRoot, ".git"), { recursive: true }),
    ]);

    const result = await runLauncher(harness, workspaceRoot, [], {
      FAKE_CANONICAL_ROOT: canonicalRoot,
      FAKE_GIT_MODE: "jj",
      FAKE_JJ_GIT_DIR: join(canonicalRoot, ".git"),
      FAKE_JJ_LOG_FAIL: "1",
      FAKE_JJ_REVISION,
    });

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain(
      "could not snapshot the current Jujutsu working copy",
    );
    expect(await captureExists(harness)).toBe(false);
  });

  test("rejects an invalid Jujutsu commit ID before invoking Nix", async () => {
    const harness = await createHarness();
    const workspaceRoot = join(harness.root, "jj-workspace");
    const canonicalRoot = join(harness.root, "canonical");
    await Promise.all([
      mkdir(workspaceRoot),
      mkdir(join(canonicalRoot, ".git"), { recursive: true }),
    ]);

    const result = await runLauncher(harness, workspaceRoot, [], {
      FAKE_CANONICAL_ROOT: canonicalRoot,
      FAKE_GIT_MODE: "jj",
      FAKE_JJ_GIT_DIR: join(canonicalRoot, ".git"),
      FAKE_JJ_REVISION: "not-a-commit",
    });

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("invalid Git commit ID");
    expect(await captureExists(harness)).toBe(false);
  });

  test("rejects caller-supplied path flake references before invoking Nix", async () => {
    const harness = await createHarness();

    const result = await runLauncher(harness, harness.root, ["path:."], {});

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("custom flake references are not accepted");
    expect(await captureExists(harness)).toBe(false);
  });

  test.each(MAINTAINED_NIX_ENTRY_POINTS)(
    "%s routes command examples through the launcher",
    async (relativePath) => {
      const contents = await readFile(
        join(REPOSITORY_ROOT, relativePath),
        "utf8",
      );

      expect(contents).toContain("./tooling/nix-develop");
      expect(contents).not.toMatch(/^[\t ]*nix develop(?:[\t ]|$)/m);
    },
  );
});

interface Harness {
  readonly root: string;
  readonly fakeBin: string;
  readonly argumentCapture: string;
  readonly pwdCapture: string;
}

async function createHarness(): Promise<Harness> {
  const root = await realpath(
    await mkdtemp(join(tmpdir(), "chenchess-nix-develop-")),
  );
  temporaryDirectories.push(root);
  const fakeBin = join(root, "bin");
  const argumentCapture = join(root, "nix-arguments");
  const pwdCapture = join(root, "nix-pwd");
  await mkdir(fakeBin);
  await Promise.all([
    writeExecutable(
      join(fakeBin, "git"),
      `#!/bin/sh
if [ "$1" != "-C" ] || [ "$3" != "rev-parse" ] || [ "$4" != "--show-toplevel" ]; then
  exit 1
fi
case "\${FAKE_GIT_MODE-}" in
  active)
    printf '%s\\n' "$FAKE_GIT_ROOT"
    ;;
  jj)
    [ "$2" = "$FAKE_CANONICAL_ROOT" ] || exit 1
    printf '%s\\n' "$FAKE_CANONICAL_ROOT"
    ;;
  *)
    exit 1
    ;;
esac
`,
    ),
    writeExecutable(
      join(fakeBin, "jj"),
      `#!/bin/sh
case "$1 $2" in
  "git root")
    printf '%s\\n' "$FAKE_JJ_GIT_DIR"
    ;;
  "log -r")
    [ "$3" = "@" ] || exit 1
    [ "$4" = "--no-graph" ] || exit 1
    [ "$5" = "-T" ] || exit 1
    [ "$6" = "commit_id" ] || exit 1
    [ "\${FAKE_JJ_LOG_FAIL-}" != "1" ] || exit 1
    printf '%s' "$FAKE_JJ_REVISION"
    ;;
  *)
    exit 1
    ;;
esac
`,
    ),
    writeExecutable(
      join(fakeBin, "nix"),
      `#!/bin/sh
: > "$NIX_ARGUMENT_CAPTURE"
for argument in "$@"; do
  printf '%s\\n' "$argument" >> "$NIX_ARGUMENT_CAPTURE"
done
printf '%s' "$PWD" > "$NIX_PWD_CAPTURE"
`,
    ),
  ]);
  return { root, fakeBin, argumentCapture, pwdCapture };
}

async function writeExecutable(path: string, content: string) {
  await writeFile(path, content);
  await chmod(path, 0o755);
}

async function runLauncher(
  harness: Harness,
  cwd: string,
  arguments_: readonly string[],
  environment: Readonly<Record<string, string>>,
) {
  const child = Bun.spawn([LAUNCHER, ...arguments_], {
    cwd,
    env: {
      ...process.env,
      ...environment,
      NIX_ARGUMENT_CAPTURE: harness.argumentCapture,
      NIX_PWD_CAPTURE: harness.pwdCapture,
      PATH: `${harness.fakeBin}:/usr/bin:/bin`,
    },
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  const [exitCode, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  return { exitCode, stdout, stderr };
}

async function capturedArguments(harness: Harness) {
  const contents = await readFile(harness.argumentCapture, "utf8");
  return contents.trimEnd().split("\n");
}

async function captureExists(harness: Harness) {
  try {
    await readFile(harness.argumentCapture);
    return true;
  } catch {
    return false;
  }
}
