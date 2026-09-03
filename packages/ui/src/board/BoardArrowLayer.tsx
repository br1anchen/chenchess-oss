import { useId } from "react"

import {
  parseBoardFile,
  parseBoardRank,
  type BoardArrow,
  type BoardInkTone,
  type BoardOrientation,
  type BoardSquare,
} from "../contracts"

/**
 * Arrow ink by tone. The values live in `theme/chenTokens.css` beside the rest
 * of the board palette — a `var()` resolves in an SVG presentation attribute
 * exactly as it does in a CSS property, so the marker and the line can name
 * the token directly.
 */
export const arrowInk = {
  candidate: "var(--color-board-arrow-candidate)",
  coach: "var(--color-board-arrow-coach)",
  engine: "var(--color-board-arrow-engine)",
  peer: "var(--color-board-arrow-peer)",
} satisfies Record<BoardInkTone, string>

const files = ["a", "b", "c", "d", "e", "f", "g", "h"] as const
const ranks = ["1", "2", "3", "4", "5", "6", "7", "8"] as const

/** The move-arrow overlay both boards share: the presentational widget board
 * and the interactive Review Session board draw the same lines. */
export function BoardArrowLayer({
  arrows,
  orientation,
}: {
  arrows: readonly BoardArrow[]
  orientation: BoardOrientation
}) {
  const markerPrefix = useId().replaceAll(":", "")
  return (
    <svg
      aria-hidden="true"
      className="coach-board-arrow-layer"
      preserveAspectRatio="none"
      viewBox="0 0 100 100"
    >
      <defs>
        {arrows.map((arrow, index) => (
          <marker
            id={`${markerPrefix}-${index}`}
            key={`${markerPrefix}-marker-${index}`}
            markerHeight="4"
            markerUnits="strokeWidth"
            markerWidth="4"
            orient="auto"
            refX="3"
            refY="2"
            viewBox="0 0 4 4"
          >
            <path d="M0 0L4 2L0 4Z" fill={arrowInk[arrow.tone]} />
          </marker>
        ))}
      </defs>
      {arrows.map((arrow, index) => {
        const endpoints = arrowEndpoints(arrow.from, arrow.to, orientation)
        return (
          <line
            data-arrow-label={arrow.label}
            key={`${markerPrefix}-line-${index}`}
            markerEnd={`url(#${markerPrefix}-${index})`}
            stroke={arrowInk[arrow.tone]}
            strokeLinecap="round"
            // The coach can draw several at once about one position, so its
            // ink lets the pieces it crosses read through. The engine draws
            // one move and stays solid.
            strokeOpacity={arrow.tone === "coach" ? 0.78 : 1}
            strokeWidth={arrow.tone === "coach" ? 1.5 : 1.8}
            x1={endpoints.x1}
            x2={endpoints.x2}
            y1={endpoints.y1}
            y2={endpoints.y2}
          />
        )
      })}
    </svg>
  )
}

export function describeBoardArrows(arrows: readonly BoardArrow[]) {
  return arrows.length > 0
    ? `Move arrows: ${arrows
        .map((arrow) => `${arrow.label}, ${arrow.from} to ${arrow.to}`)
        .join("; ")}.`
    : "No move arrows shown."
}

function arrowEndpoints(
  from: BoardSquare,
  to: BoardSquare,
  orientation: BoardOrientation,
) {
  const start = squareCenter(from, orientation)
  const end = squareCenter(to, orientation)
  const deltaX = end.x - start.x
  const deltaY = end.y - start.y
  const length = Math.hypot(deltaX, deltaY) || 1
  const inset = 2.4
  return {
    x1: rounded(start.x),
    x2: rounded(end.x - (deltaX / length) * inset),
    y1: rounded(start.y),
    y2: rounded(end.y - (deltaY / length) * inset),
  }
}

export function squareCenter(
  square: BoardSquare,
  orientation: BoardOrientation,
) {
  const file = files.indexOf(parseBoardFile(square[0]))
  const rank = ranks.indexOf(parseBoardRank(square[1]))
  return orientation === "white"
    ? { x: (file + 0.5) * 12.5, y: (7 - rank + 0.5) * 12.5 }
    : { x: (7 - file + 0.5) * 12.5, y: (rank + 0.5) * 12.5 }
}

function rounded(value: number) {
  return Math.round(value * 10) / 10
}
