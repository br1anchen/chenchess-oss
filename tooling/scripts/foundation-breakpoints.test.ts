import { expect, test } from "bun:test";
import { glob, readFile } from "node:fs/promises";
import { join, resolve } from "node:path";

const ROOT = resolve(import.meta.dir, "../..");

/**
 * The five foundation cuts live in `packages/ui/src/theme/breakpoints.ts`, but
 * `theme/breakpoints.ts` cannot be imported into a `stylex.create` the Coach App
 * artifact build compiles, so every StyleX file copies the literal into a local
 * `const`. CODING_STANDARDS names the defect that follows: copying a *different*
 * width. This reads the widths back out of the source of truth and holds every
 * copy to them — the half of "name a foundation breakpoint" a script can judge.
 * Whether the local is *named* after the cut it copies stays a review rule.
 */
const SOURCE_OF_TRUTH = "packages/ui/src/theme/breakpoints.ts";

/**
 * The one documented partner to a cut rather than a sixth cut: it undoes the
 * 64rem stack cut, so it is the 64rem cut read the other way.
 */
const STACK_COMPLEMENT = "@media (min-width: 64.01rem)";

const SCANNED = [
  "apps/*/src/**/*.{ts,tsx,css,astro}",
  "apps/*/*.{ts,tsx,css,astro}",
  "packages/*/src/**/*.{ts,tsx,css,astro}",
  "packages/*/stories/**/*.{ts,tsx,css,astro}",
];

async function foundationWidths() {
  const source = await readFile(join(ROOT, SOURCE_OF_TRUTH), "utf8");
  const widths = new Set(
    [...source.matchAll(/@media \(max-width: ([^)]+)\)/g)].map(
      (match) => match[1],
    ),
  );
  expect(widths.size).toBe(5);
  return widths;
}

async function* scannedFiles() {
  const seen = new Set<string>();
  for (const pattern of SCANNED) {
    for await (const relative of glob(pattern, { cwd: ROOT })) {
      if (relative.includes("node_modules")) continue;
      // Generated stylesheets are emitted from the kit, not authored.
      if (relative.includes("/generated/")) continue;
      if (relative === SOURCE_OF_TRUTH) continue;
      if (seen.has(relative)) continue;
      seen.add(relative);
      yield relative;
    }
  }
}

test("every viewport query names a foundation breakpoint", async () => {
  const widths = await foundationWidths();
  const strays: string[] = [];
  for await (const relative of scannedFiles()) {
    const text = await readFile(join(ROOT, relative), "utf8");
    for (const match of text.matchAll(/@media \(max-width: ([^)]+)\)/g)) {
      if (widths.has(match[1])) continue;
      strays.push(`${relative}: max-width ${match[1]}`);
    }
    for (const match of text.matchAll(/@media \(min-width: [^)]+\)/g)) {
      if (match[0] === STACK_COMPLEMENT) continue;
      strays.push(`${relative}: ${match[0]}`);
    }
  }
  expect(strays).toEqual([]);
});

/**
 * The move-nav cut is the one #418 D1 mandates, so its copy is the one worth
 * naming outright: a Player below it gets an icon-only nav, and a width that
 * drifted here would wrap the nav on exactly the phones the cut exists for.
 */
test("the move-nav compact cut is copied at the mandated width", async () => {
  const styles = await readFile(
    join(ROOT, "packages/ui/src/components/watercolor.styles.ts"),
    "utf8",
  );
  expect(styles).toContain('const compactNav = "@media (max-width: 520px)"');
  const source = await readFile(join(ROOT, SOURCE_OF_TRUTH), "utf8");
  expect(source).toContain('moveNav: "@media (max-width: 520px)"');
});
