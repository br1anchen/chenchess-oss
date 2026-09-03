import { readFile } from "node:fs/promises"
import { resolve } from "node:path"

const defaultBundle = resolve(import.meta.dirname, "../server-dist/server.js")

/**
 * A workspace package left external in the server bundle is a container that
 * will not start.
 *
 * The bundle is built with `--packages=external`, and the runtime image copies
 * `/app/node_modules` without `/app/packages`, so every `@chenchess/*` symlink
 * in it dangles. The aliases in `build:server` exist to inline those packages
 * from source; a workspace package added without one bundles clean, passes
 * every local gate, and then dies at startup with `ERR_MODULE_NOT_FOUND` — the
 * healthcheck is the first thing that notices.
 *
 * `@chenchess/review-projection` did exactly that in #258. This is the check
 * that would have caught it, so it runs where the bundle is produced rather
 * than where a deployment fails.
 */
export async function verifyServerBundle(bundlePath = defaultBundle) {
  const bundle = await readFile(bundlePath, "utf8")
  const external = [
    ...new Set(bundle.match(/@chenchess\/[a-z0-9-]+/g) ?? []),
  ].sort()
  if (external.length > 0) {
    throw new Error(
      `the server bundle imports ${external.join(", ")} as external packages, ` +
        "which the runtime image cannot resolve. Add an " +
        "`--alias:<package>=<path to its src>` to `build:server` for each.",
    )
  }
}

if (import.meta.main) {
  try {
    await verifyServerBundle()
    process.stdout.write(
      "verified server bundle has no external workspace imports\n",
    )
  } catch (error) {
    process.stderr.write(
      `${error instanceof Error ? error.message : "server bundle verification failed"}\n`,
    )
    process.exitCode = 1
  }
}
