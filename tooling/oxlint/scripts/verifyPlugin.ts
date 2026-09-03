import { mkdtemp, readFile, rm } from "node:fs/promises"
import { tmpdir } from "node:os"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const pluginPath = join(root, "plugin.js")
const indexPath = join(root, "index.ts")

const tempDir = await mkdtemp(join(tmpdir(), "oxlint-plugin-"))
const generatedPath = join(tempDir, "plugin.js")
const build = Bun.spawn(
  [
    "bun",
    "build",
    indexPath,
    "--outfile",
    generatedPath,
    "--target=node",
    "--format=esm",
    "--packages=external",
  ],
  { cwd: root, stdout: "pipe", stderr: "pipe" },
)
const stderr = await new Response(build.stderr).text()
const exitCode = await build.exited
if (exitCode !== 0) {
  throw new Error(stderr || "oxlint plugin bundle failed")
}

const committed = await readFile(pluginPath)
const generated = await readFile(generatedPath)
await rm(tempDir, { recursive: true, force: true })
if (!committed.equals(generated)) {
  throw new Error(
    "tooling/oxlint/plugin.js is stale; run bun run --cwd tooling/oxlint build",
  )
}
process.stdout.write(
  "verified tooling/oxlint/plugin.js matches a fresh bundle\n",
)
