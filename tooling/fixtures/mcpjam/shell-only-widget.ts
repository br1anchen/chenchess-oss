import { App } from "@modelcontextprotocol/ext-apps"

type ShellState =
  | { readonly kind: "loading"; readonly visibleText: "Loading game..." }
  | { readonly kind: "no-game"; readonly visibleText: "No game loaded" }
  | { readonly kind: "error"; readonly visibleText: "Could not load game" }

const shellState = {
  kind: "no-game",
  visibleText: "No game loaded",
} satisfies ShellState

const app = new App(
  { name: "mcpjam-shell-only-repro", version: "1.0.0" },
  {},
  { autoResize: false, strict: true },
)

function render(state: ShellState): void {
  const heading = document.querySelector<HTMLElement>("[data-shell-state]")
  if (!heading) throw new Error("The shell state element is missing.")

  switch (state.kind) {
    case "loading":
    case "no-game":
    case "error":
      heading.dataset.semanticState = state.kind
      heading.textContent = state.visibleText
      return
    default: {
      const unhandled: never = state
      throw new Error(`Unhandled shell state: ${String(unhandled)}`)
    }
  }
}

async function main(): Promise<void> {
  render(shellState)
  // Register the notification handlers before connect, as the Apps SDK
  // requires. They intentionally preserve the non-meaningful shell state.
  app.ontoolinput = () => render(shellState)
  app.ontoolresult = () => render(shellState)
  await app.connect()
  document.documentElement.dataset.bridgeInitialized = "true"
}

void main().catch((error: unknown) => {
  console.error(error)
})
