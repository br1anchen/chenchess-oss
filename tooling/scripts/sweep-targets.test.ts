import { describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import {
  incrementalDirectories,
  isLinkedJjWorkspace,
  isSweepDue,
  nextSweepDue,
  parseCleanedBytes,
  parseGitWorktreePaths,
  parseOptions,
  parseStamp,
  readStamp,
  selectSweepRoots,
  stampPath,
  SweepFailure,
  sweepArguments,
  writeStamp,
} from "./sweep-targets";

const DAY = 24 * 60 * 60 * 1000;
const NOW = new Date("2026-08-07T12:00:00.000Z");
const ROUTINE = { days: 3, intervalDays: 7, force: false };

describe("sweep options", () => {
  test("defaults to a three-day retention without deleting incremental state", () => {
    expect(parseOptions([])).toEqual({
      days: 3,
      dryRun: false,
      force: false,
      intervalDays: 7,
      maxSize: null,
      purgeIncremental: false,
    });
  });

  test("accepts both separated and joined forms", () => {
    expect(parseOptions(["--days", "7", "--max-size=20GB"])).toEqual({
      days: 7,
      dryRun: false,
      force: false,
      intervalDays: 7,
      maxSize: "20GB",
      purgeIncremental: false,
    });
    expect(
      parseOptions([
        "--days=0",
        "--dry-run",
        "--purge-incremental",
        "--interval-days=1",
        "--force",
      ]),
    ).toEqual({
      days: 0,
      dryRun: true,
      force: true,
      intervalDays: 1,
      maxSize: null,
      purgeIncremental: true,
    });
  });

  test("rejects an unknown argument and an unusable retention", () => {
    expect(() => parseOptions(["--everything"])).toThrow(SweepFailure);
    expect(() => parseOptions(["--days", "-1"])).toThrow(SweepFailure);
    expect(() => parseOptions(["--days", "week"])).toThrow(SweepFailure);
    expect(() => parseOptions(["--max-size", "huge"])).toThrow(SweepFailure);
    expect(() => parseOptions(["--interval-days", "-1"])).toThrow(SweepFailure);
  });

  test("names the offending flag when the interval is unusable", () => {
    expect(() => parseOptions(["--interval-days", "week"])).toThrow(
      "--interval-days requires a non-negative whole number",
    );
  });
});

describe("sweep interval", () => {
  test("sweeps a root that has never been swept", () => {
    expect(isSweepDue(null, NOW, ROUTINE)).toBe(true);
  });

  test("skips a root swept inside the interval", () => {
    const stamp = {
      sweptAt: new Date(NOW.getTime() - 2 * DAY).toISOString(),
      days: 3,
    };
    expect(isSweepDue(stamp, NOW, ROUTINE)).toBe(false);
  });

  test("sweeps once the interval has elapsed, boundary included", () => {
    for (const age of [7, 8, 30]) {
      const stamp = {
        sweptAt: new Date(NOW.getTime() - age * DAY).toISOString(),
        days: 3,
      };
      expect(isSweepDue(stamp, NOW, ROUTINE)).toBe(true);
    }
  });

  test("--force overrides a fresh stamp", () => {
    const stamp = { sweptAt: NOW.toISOString(), days: 3 };
    expect(isSweepDue(stamp, NOW, { ...ROUTINE, force: true })).toBe(true);
  });

  test("sweeps when the request is stricter than the recorded retention", () => {
    const stamp = {
      sweptAt: new Date(NOW.getTime() - 1 * DAY).toISOString(),
      days: 30,
    };
    expect(isSweepDue(stamp, NOW, { ...ROUTINE, days: 3 })).toBe(true);
    expect(isSweepDue(stamp, NOW, { ...ROUTINE, days: 30 })).toBe(false);
  });

  test("distrusts a stamp dated in the future", () => {
    const stamp = {
      sweptAt: new Date(NOW.getTime() + 5 * DAY).toISOString(),
      days: 3,
    };
    expect(isSweepDue(stamp, NOW, ROUTINE)).toBe(true);
  });

  test("reports the next due date from the recorded sweep", () => {
    const stamp = { sweptAt: NOW.toISOString(), days: 3 };
    expect(nextSweepDue(stamp, 7).toISOString()).toBe(
      new Date(NOW.getTime() + 7 * DAY).toISOString(),
    );
  });
});

describe("sweep stamp", () => {
  test("treats a malformed or unparseable stamp as absent", () => {
    expect(parseStamp("not json")).toBeNull();
    expect(parseStamp("[]")).toBeNull();
    expect(parseStamp('{"days":3}')).toBeNull();
    expect(parseStamp('{"sweptAt":"whenever","days":3}')).toBeNull();
    expect(parseStamp('{"sweptAt":"2026-08-01T00:00:00.000Z"}')).toBeNull();
  });

  test("round-trips through the target directory", () => {
    const root = mkdtempSync(join(tmpdir(), "sweep-stamp-"));
    mkdirSync(join(root, "target"), { recursive: true });
    expect(readStamp(root)).toBeNull();

    const stamp = { sweptAt: NOW.toISOString(), days: 3 };
    writeStamp(root, stamp);

    expect(stampPath(root)).toBe(
      join(root, "target", ".chenchess-sweep-stamp.json"),
    );
    expect(readStamp(root)).toEqual(stamp);
    expect(isSweepDue(readStamp(root), NOW, ROUTINE)).toBe(false);
  });

  test("a corrupted stamp makes the root due rather than stuck", () => {
    const root = mkdtempSync(join(tmpdir(), "sweep-stamp-"));
    mkdirSync(join(root, "target"), { recursive: true });
    writeFileSync(stampPath(root), "{ truncated");

    expect(readStamp(root)).toBeNull();
    expect(isSweepDue(readStamp(root), NOW, ROUTINE)).toBe(true);
  });
});

describe("workspace discovery", () => {
  test("reads checkout paths from Git worktree porcelain output", () => {
    const porcelain = [
      "worktree /repos/chenchess",
      "HEAD 1ca4d780d373a8e00b9ff8dec4efce0641c78314",
      "detached",
      "",
      "worktree /repos/chenchess-217",
      "HEAD 2b31e060d373a8e00b9ff8dec4efce0641c78314",
      "branch refs/heads/217",
      "",
    ].join("\n");

    expect(parseGitWorktreePaths(porcelain)).toEqual([
      "/repos/chenchess",
      "/repos/chenchess-217",
    ]);
  });

  test("recognizes only Jujutsu workspaces backed by this repository", () => {
    expect(
      isLinkedJjWorkspace("/repos/chenchess/.jj/repo\n", "/repos/chenchess"),
    ).toBe(true);
    expect(
      isLinkedJjWorkspace("/repos/other/.jj/repo", "/repos/chenchess"),
    ).toBe(false);
  });

  test("keeps the checkout and its siblings, dropping anything else", () => {
    const roots = selectSweepRoots(
      "/repos/chenchess",
      [
        "/repos/chenchess",
        "/repos/chenchess-217",
        "/repos/chenchess/nested",
        "/elsewhere/chenchess-999",
        "/repos/not-rust",
      ],
      (path) => path !== "/repos/not-rust",
    );

    expect(roots).toEqual(["/repos/chenchess", "/repos/chenchess-217"]);
  });

  test("drops a sibling that is not a Cargo project", () => {
    expect(
      selectSweepRoots("/repos/chenchess", ["/repos/docs-site"], () => false),
    ).toEqual([]);
  });
});

describe("cargo-sweep invocation", () => {
  test("passes every root to one retention-scoped invocation", () => {
    expect(
      sweepArguments({
        roots: ["/repos/chenchess", "/repos/chenchess-217"],
        options: {
          days: 3,
          dryRun: false,
          force: false,
          intervalDays: 7,
          maxSize: null,
          purgeIncremental: false,
        },
      }),
    ).toEqual([
      "sweep",
      "--time",
      "3",
      "/repos/chenchess",
      "/repos/chenchess-217",
    ]);
  });

  test("adds the size bound and the dry run before the roots", () => {
    expect(
      sweepArguments({
        roots: ["/repos/chenchess"],
        options: {
          days: 7,
          dryRun: true,
          force: false,
          intervalDays: 7,
          maxSize: "20GB",
          purgeIncremental: false,
        },
      }),
    ).toEqual([
      "sweep",
      "--time",
      "7",
      "--maxsize",
      "20GB",
      "--dry-run",
      "/repos/chenchess",
    ]);
  });

  test("sums the per-root totals of a real run", () => {
    expect(
      parseCleanedBytes(
        [
          '[INFO] Cleaned 30.32 GiB from "/repos/chenchess/target"',
          '[INFO] Cleaned 512 MiB from "/repos/chenchess-217/target"',
        ].join("\n"),
      ),
    ).toBe(30.32 * 1024 ** 3 + 512 * 1024 ** 2);
  });

  test("reads the dry-run total, tolerating its absence", () => {
    expect(
      parseCleanedBytes('[INFO] Would clean: 1.5 GiB from "/repos/chenchess"'),
    ).toBe(1.5 * 1024 ** 3);
    expect(parseCleanedBytes("nothing to report")).toBeNull();
    expect(
      parseCleanedBytes('[WARN] Failed to clean "/repos/x/target"'),
    ).toBeNull();
  });
});

describe("incremental purge", () => {
  test("finds an incremental directory under every profile", () => {
    const root = mkdtempSync(join(tmpdir(), "sweep-targets-"));
    mkdirSync(join(root, "target/debug/incremental"), { recursive: true });
    mkdirSync(join(root, "target/release/incremental"), { recursive: true });
    mkdirSync(join(root, "target/debug/deps"), { recursive: true });
    writeFileSync(join(root, "target/CACHEDIR.TAG"), "");

    expect(incrementalDirectories(root).sort()).toEqual([
      resolve(root, "target/debug/incremental"),
      resolve(root, "target/release/incremental"),
    ]);
  });

  test("reports nothing when the checkout has never been built", () => {
    expect(
      incrementalDirectories(mkdtempSync(join(tmpdir(), "sweep-"))),
    ).toEqual([]);
  });
});
