import { App } from "@modelcontextprotocol/ext-apps"

import { inertDependencyMethodName } from "./inert-sdk-dependency"

const app = new App(
  { name: "mcpjam-false-download-repro", version: "1.0.0" },
  {},
  { autoResize: false, strict: true },
)

async function main(): Promise<void> {
  const status = document.querySelector<HTMLElement>("[data-repro-status]")
  if (!status) throw new Error("The reproduction status element is missing.")

  // Keep the inert dependency string in the final bundle. Reading a method
  // name is deliberately not the same thing as invoking that host method.
  status.dataset.inertDependencyMethod = inertDependencyMethodName()

  await app.connect()
  status.dataset.bridgeInitialized = "true"
  status.textContent = "Bridge initialized. No file operation was requested."
}

void main().catch((error: unknown) => {
  const status = document.querySelector<HTMLElement>("[data-repro-status]")
  if (status) status.textContent = "Bridge initialization failed."
  console.error(error)
})
