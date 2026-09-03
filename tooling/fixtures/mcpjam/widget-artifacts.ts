import { build } from "esbuild"

import {
  falseDownloadResourceUri,
  shellOnlyResourceUri,
} from "./repro-contract"

export type WidgetArtifact = {
  readonly html: string
  readonly resourceUri: string
}

export type ReproWidgetArtifacts = {
  readonly falseDownload: WidgetArtifact
  readonly shellOnly: WidgetArtifact
}

export async function buildReproWidgetArtifacts(): Promise<ReproWidgetArtifacts> {
  const [falseDownload, shellOnly] = await Promise.all([
    buildWidget({
      entryPoint: new URL("./false-download-widget.ts", import.meta.url)
        .pathname,
      resourceUri: falseDownloadResourceUri,
      title: "False download inference control",
      body: "<p data-repro-status>Connecting to the host bridge...</p>",
    }),
    buildWidget({
      entryPoint: new URL("./shell-only-widget.ts", import.meta.url).pathname,
      resourceUri: shellOnlyResourceUri,
      title: "Shell-only render control",
      body: [
        '<h1 data-shell-state data-semantic-state="no-game">No game loaded</h1>',
        "<p>This painted shell has no durable game address or review content.</p>",
      ].join(""),
    }),
  ])
  return { falseDownload, shellOnly }
}

type WidgetBuildInput = {
  readonly body: string
  readonly entryPoint: string
  readonly resourceUri: string
  readonly title: string
}

async function buildWidget(input: WidgetBuildInput): Promise<WidgetArtifact> {
  const result = await build({
    bundle: true,
    charset: "utf8",
    entryPoints: [input.entryPoint],
    format: "iife",
    legalComments: "none",
    minify: false,
    platform: "browser",
    target: "es2022",
    write: false,
  })
  const output = result.outputFiles[0]
  if (!output) throw new Error(`esbuild produced no ${input.title} bundle.`)

  return {
    resourceUri: input.resourceUri,
    html: htmlDocument({
      body: input.body,
      script: output.text,
      title: input.title,
    }),
  }
}

function htmlDocument(input: {
  readonly body: string
  readonly script: string
  readonly title: string
}): string {
  const script = input.script.replaceAll("</script", "<\\/script")
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>${input.title}</title>
    <style>
      :root { color-scheme: light dark; font: 16px/1.45 system-ui, sans-serif; }
      body { margin: 0; padding: 24px; background: Canvas; color: CanvasText; }
      main { min-height: 120px; border: 1px solid GrayText; border-radius: 12px; padding: 20px; }
      h1 { margin: 0 0 8px; font-size: 20px; }
      p { margin: 0; }
    </style>
  </head>
  <body>
    <main>${input.body}</main>
    <script>${script}</script>
  </body>
</html>`
}
