import { type BoardOrientation, type BoardSquareMark } from "../contracts"

import { arrowInk, squareCenter } from "./BoardArrowLayer"

/**
 * The squares the board has singled out, drawn under the arrows.
 *
 * A mark is a tinted square rather than a line because what it points at is
 * a place, not a move. It shares the arrow ink so one tone means one source
 * across the whole overlay.
 */
export function BoardMarkLayer({
  marks,
  orientation,
}: {
  marks: readonly BoardSquareMark[]
  orientation: BoardOrientation
}) {
  return (
    <svg
      aria-hidden="true"
      className="coach-board-mark-layer"
      preserveAspectRatio="none"
      viewBox="0 0 100 100"
    >
      {marks.map((mark) => {
        const center = squareCenter(mark.square, orientation)
        return (
          <rect
            data-mark-label={mark.label}
            fill={arrowInk[mark.tone]}
            fillOpacity="0.42"
            height="11.5"
            key={`${mark.square}-${mark.tone}`}
            rx="1.4"
            stroke={arrowInk[mark.tone]}
            strokeOpacity="1"
            strokeWidth="1.2"
            width="11.5"
            x={center.x - 5.75}
            y={center.y - 5.75}
          />
        )
      })}
    </svg>
  )
}

export function describeBoardMarks(marks: readonly BoardSquareMark[]) {
  return `Marked squares: ${marks
    .map((mark) => `${mark.label}, ${mark.square}`)
    .join("; ")}.`
}
