# MCPJam compatibility reproductions

These fixtures isolate two negative controls for MCPJam Inspector 2.35.0. They
use recorded text only, bind to loopback, and need no credentials or Player
data. They do not replace the strict-modern enabled/disabled artifact proof in
`bun run check:mcp-e2e` or hosted evidence from `review-session:journeys`.

The pinned upstream scanner searches the whole widget HTML for
`ui/download-file`, `getFileDownloadUrl`, or `uploadFile`. It explicitly uses
string matching and therefore also sees method definitions bundled from a
dependency. The source is
[`widget-scan.ts`](https://github.com/MCPJam/inspector/blob/8d631f480933b846eeb215a9cef2442016c76ed9/sdk/src/host-compat/widget-scan.ts).

The pinned renderer reports `rendered` when the MCP Apps bridge initialized and
the screenshot differs from the blank host frame. It does not decide whether
the pixels contain useful application data. The classification branch is in
[`mcp-app-browser-harness.ts`](https://github.com/MCPJam/inspector/blob/8d631f480933b846eeb215a9cef2442016c76ed9/mcpjam-inspector/server/utils/mcp-app-browser-harness.ts#L923-L975).

## Executable contract

Run the source-contract tests:

```bash
./tooling/nix-develop --command bun test tooling/fixtures/mcpjam
```

The helper is reusable by the compatibility producer:

```ts
import { openPinnedMcpJamBrowserHarness } from "@chenchess/tooling-fixtures/mcpjam/upstream-browser-harness"
```

It accepts supplied widget HTML, the exact mounting Tool definition, tool
input/output, permissions, CSP metadata, and an app-to-host `callTool`
callback. It returns bounded DOM text, screenshot metadata, outbound method
names, and app tool-call summaries without exposing tool arguments in its
report. `bun run check:mcp-e2e` uses this helper for the telemetry-enabled and
telemetry-disabled artifacts over the same strict-modern connection. The gate
builds its compatibility observation from each live connection and browser
mount rather than accepting a caller-authored evidence file.

## Live MCPJam runs

Start the reproduction server in one terminal:

```bash
./tooling/nix-develop --command bun tooling/fixtures/mcpjam/server.ts
```

Start the repository-pinned Inspector in a second terminal:

```bash
./tooling/nix-develop --command bun run inspect:mcp
```

Then run the pinned CLI from a third terminal. The offline compatibility report
should infer a file-download requirement for `show_false_download_control`,
despite the static report above proving that no permission or app call exists.

```bash
./tooling/nix-develop --command npm exec --yes -- @mcpjam/cli@3.19.0 compat --offline --host chatgpt --url http://127.0.0.1:5175/mcp
```

The CLI render path remains useful for checking a future upstream fix. Its PNG
should say `No game loaded`, and the fixture's typed state remains `not-ready`.

```bash
./tooling/nix-develop --command npm exec --yes -- @mcpjam/cli@3.19.0 apps render --url http://127.0.0.1:5175/mcp --tool-name show_shell_only --tool-args '{}' --protocol mcp-apps --viewport 800x500 --require-render --screenshot-out /tmp/mcpjam-shell-only.png
```

With the repository's current pins, the CLI stops before rendering with
`projectId is required`. Inspector 2.35.0 removed the legacy local
`{ serverConfig }` connect body, while CLI 3.19.0 still sends it. The same-day
CLI 3.15.2, Inspector 2.29.0, and SDK 2.0.0 release set has the same connect
contract mismatch.
