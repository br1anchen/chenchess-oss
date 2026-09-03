import type { ReviewSide } from "@chenchess/coach-engine-sdk"
import { Icon } from "@chenchess/ui/astryx"
import {
  Banner,
  Grid,
  HStack,
  VStack,
  WatercolorButton,
  WatercolorCard,
  WatercolorField,
  WatercolorInput,
  WatercolorSelect,
} from "@chenchess/ui"
import type { ReactNode } from "react"

import { importedGamesStyles } from "./dashboardWorkspace.styles"
import { type ClipboardEvent, type FormEvent, useState } from "react"

import {
  parseImportGameRequest,
  preselectedReviewSide,
  type ParsedImportGameRequest,
} from "./importGameRequest"

export type ReadyImportGameRequest = Extract<
  ParsedImportGameRequest,
  { kind: "ready" }
>

const reviewSideOptions = [
  { label: "White", value: "white" },
  { label: "Black", value: "black" },
  { label: "Both sides (pasted PGN)", value: "both" },
]

export function ImportGameCard({
  busy,
  failure,
  header,
  onImport,
  progress,
}: {
  busy: boolean
  failure: string | null
  header?: ReactNode
  onImport: (request: ReadyImportGameRequest) => void
  progress: string | null
}) {
  const [source, setSource] = useState("")
  const [reviewSide, setReviewSide] = useState<ReviewSide>("white")
  const [elo, setElo] = useState("")
  const [invalid, setInvalid] = useState<string | null>(null)
  const message = invalid ?? failure

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (busy) return
    const request = parseImportGameRequest({ elo, reviewSide, source })
    if (request.kind === "invalid") {
      setInvalid(request.message)
      return
    }
    setInvalid(null)
    onImport(request)
  }

  // A side-qualified Lichess URL preselects its Review Side and the control stays
  // authoritative from there, so this adopts on every source change rather than
  // once: editing the URL is the Player choosing a side again.
  function changeSource(value: string) {
    setSource(value)
    const preselected = preselectedReviewSide(value)
    if (preselected) setReviewSide(preselected)
  }

  // A single-line field would otherwise lose a pasted PGN's line breaks to the
  // browser, which strips them without promising a space in their place — and
  // `e5\n2. Nf3` arriving as `e52. Nf3` is a game the Engine cannot read. Taking
  // the paste verbatim keeps the exact bytes the Player copied; only the display
  // is flattened. Splicing at the caret keeps the ordinary editing contract that
  // replacing the whole field would break.
  function pasteGame(event: ClipboardEvent<HTMLElement>) {
    const pasted = event.clipboardData.getData("text")
    if (!pasted.includes("\n")) return
    event.preventDefault()
    const field = event.target
    if (!(field instanceof HTMLInputElement)) return
    const start = field.selectionStart ?? field.value.length
    const end = field.selectionEnd ?? field.value.length
    changeSource(
      `${field.value.slice(0, start)}${pasted}${field.value.slice(end)}`,
    )
  }

  return (
    <WatercolorCard
      padding="compact"
      tone="mist"
      xstyle={importedGamesStyles.card}
    >
      <form onSubmit={submit}>
        <VStack gap={3} hAlign="stretch">
          {header}
          <WatercolorField label="Game URL or PGN">
            <WatercolorInput
              disabled={busy}
              name="gameSource"
              onChange={(event) => changeSource(event.target.value)}
              onPaste={pasteGame}
              placeholder="Paste a Chess.com or Lichess game URL, or a full PGN…"
              value={flattened(source)}
            />
          </WatercolorField>
          <HStack xstyle={importedGamesStyles.fields}>
            <Grid columns={2} xstyle={importedGamesStyles.pair}>
              <VStack xstyle={importedGamesStyles.field}>
                <WatercolorField label="Review side">
                  <WatercolorSelect
                    disabled={busy}
                    name="reviewSide"
                    onChange={(event) =>
                      setReviewSide(parseReviewSide(event.target.value))
                    }
                    value={reviewSide}
                  >
                    {reviewSideOptions.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </WatercolorSelect>
                </WatercolorField>
              </VStack>
              <VStack xstyle={importedGamesStyles.field}>
                <WatercolorField label="Elo (optional)">
                  <WatercolorInput
                    disabled={busy}
                    name="elo"
                    onChange={(event) => setElo(event.target.value)}
                    placeholder="From the game"
                    value={elo}
                  />
                </WatercolorField>
              </VStack>
            </Grid>
            <WatercolorButton disabled={busy} loading={busy} type="submit">
              {busy ? (
                <Icon icon="loader" size="sm" />
              ) : (
                <Icon icon="plus" size="sm" />
              )}
              Import
            </WatercolorButton>
          </HStack>
          {message ? (
            <Banner
              description={message}
              status="error"
              title="Import failed"
            />
          ) : null}
          {progress ? (
            <Banner
              description={progress}
              icon={<Icon icon="loader" size="sm" />}
              status="info"
              title="Importing"
            />
          ) : null}
        </VStack>
      </form>
    </WatercolorCard>
  )
}

function parseReviewSide(value: string): ReviewSide {
  switch (value) {
    case "white":
    case "black":
    case "both":
      return value
    default:
      throw new TypeError("invalid import Review Side")
  }
}

/**
 * One line of the Player's source, for display only.
 *
 * Flattening runs of whitespace is safe to read but lossy to keep: PGN treats
 * every whitespace run as one separator, so the field can show a pasted game on
 * one line while `source` still holds what the Engine parses.
 *
 * Editing after a paste does commit the flattened bytes, and one PGN shape does
 * not survive that: a `;` comment runs to end of line, so flattened it swallows
 * the rest of the game. Neither provider exports those, and stripping `;` runs
 * would corrupt a tag value containing one, so this is a documented edge rather
 * than a guard.
 */
function flattened(source: string): string {
  return source.includes("\n") ? source.replace(/\s+/g, " ").trim() : source
}
