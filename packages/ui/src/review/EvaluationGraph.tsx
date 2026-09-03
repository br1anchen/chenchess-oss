import { Text } from "@astryxdesign/core/Text"
import * as stylex from "@stylexjs/stylex"
import { useId } from "react"

import { graphStyles, momentToneStyles } from "./ReviewNavigation.styles"
import {
  evaluationAt,
  type EvaluationPointPresentation,
  type ReviewContextNavigationProps,
} from "./reviewNavigationPresentation"

/** Keeps a structural class hook alongside the compiled StyleX classes. */
function craft(
  hook: string,
  ...styles: ReadonlyArray<object | false | null | undefined>
) {
  // SAFETY: every argument is compiled StyleX from ReviewNavigation.styles.ts;
  // the published prop types cannot express the authored style objects.
  const applied = stylex.props(...(styles as never[]))
  return {
    ...applied,
    className: [hook, applied.className].filter(Boolean).join(" "),
  }
}

export function EvaluationGraph({
  activePly,
  caption = true,
  density = "default",
  disabled,
  maxPly,
  moments,
  onSelect,
  points,
  skin = "plain",
  title = "Real-game evaluation",
}: ReviewContextNavigationProps & {
  caption?: boolean
  /** `sparkline` is the compact whole-game plot under a Review Session board. */
  density?: "default" | "sparkline"
  maxPly: number
  points: readonly EvaluationPointPresentation[]
  /** `watercolor` is the plot as the review card paints it: ink-dot moment
   * markers on the card's own paper, no plot chrome of its own. */
  skin?: "plain" | "watercolor"
  title?: string | null
}) {
  const watercolor = skin === "watercolor"
  const sparkline = density === "sparkline"
  const gradientId = useId().replaceAll(":", "")
  const safeMaxPly = Math.max(1, maxPly)
  const sorted = [...points].sort((left, right) => left.ply - right.ply)
  const graphPoints = sorted
    .map((point) => `${graphX(point.ply, safeMaxPly)},${graphY(point.value)}`)
    .join(" ")
  const areaPoints = graphPoints ? `7,94 ${graphPoints} 93,94` : ""
  const markerX = graphX(activePly, safeMaxPly)
  const activeEvaluation = evaluationAt(sorted, activePly)
  const orderedMoments = [...moments].sort(
    (left, right) => left.ply - right.ply,
  )
  const momentHitRegions = new Map(
    orderedMoments.map((moment, index) => {
      const x = graphX(moment.ply, safeMaxPly)
      const previousX =
        index === 0 ? 0 : graphX(orderedMoments[index - 1]!.ply, safeMaxPly)
      const nextX =
        index === orderedMoments.length - 1
          ? 100
          : graphX(orderedMoments[index + 1]!.ply, safeMaxPly)
      const left = index === 0 ? 0 : (previousX + x) / 2
      const right = index === orderedMoments.length - 1 ? 100 : (x + nextX) / 2

      return [
        moment.ply,
        {
          left,
          markerLeft: ((x - left) / (right - left)) * 100,
          width: right - left,
        },
      ] as const
    }),
  )

  return (
    <figure
      {...craft(
        "chen-review-evaluation-graph",
        graphStyles.figure,
        watercolor && graphStyles.figureWatercolor,
      )}
    >
      {caption ? (
        <figcaption {...stylex.props(graphStyles.caption)}>
          {title ? (
            <Text type="body" weight="semibold">
              {title}
            </Text>
          ) : null}
          <Text
            aria-label="Evaluation at the selected moment"
            role="status"
            type="supporting"
          >
            {activeEvaluation?.label ?? "—"}
          </Text>
        </figcaption>
      ) : null}
      <div
        {...craft(
          "chen-review-graph-plot",
          graphStyles.plot,
          watercolor && graphStyles.plotWatercolor,
          sparkline && graphStyles.plotSparkline,
        )}
      >
        <svg
          aria-label="Measured real-game evaluation graph"
          preserveAspectRatio="none"
          role="img"
          viewBox="0 0 100 100"
          {...stylex.props(
            graphStyles.svg,
            watercolor && graphStyles.svgWatercolor,
          )}
        >
          <defs>
            <linearGradient id={gradientId} x1="0" x2="0" y1="0" y2="1">
              <stop
                offset="0%"
                stopColor="var(--color-border)"
                stopOpacity="0.72"
              />
              <stop
                offset="100%"
                stopColor="var(--color-border)"
                stopOpacity="0.08"
              />
            </linearGradient>
          </defs>
          <line
            x1="7"
            x2="93"
            y1="50"
            y2="50"
            {...craft("chen-review-graph-zero", graphStyles.zero)}
          />
          {areaPoints ? (
            <polygon fill={`url(#${gradientId})`} points={areaPoints} />
          ) : null}
          {graphPoints ? (
            <polyline
              points={graphPoints}
              {...stylex.props(graphStyles.line)}
            />
          ) : null}
          {sorted.map((point) => (
            <circle
              cx={graphX(point.ply, safeMaxPly)}
              cy={graphY(point.value)}
              key={point.ply}
              r="1.15"
              {...stylex.props(graphStyles.point)}
            >
              <title>{`Ply ${point.ply}: ${point.label}`}</title>
            </circle>
          ))}
          <line
            x1={markerX}
            x2={markerX}
            y1="6"
            y2="94"
            {...craft(
              "chen-review-graph-marker",
              graphStyles.marker,
              watercolor && graphStyles.markerWatercolor,
            )}
          />
        </svg>
        {moments.map((moment) => {
          const evaluation = evaluationAt(sorted, moment.ply)
          const hitRegion = momentHitRegions.get(moment.ply)
          const active = moment.ply === activePly
          const button = craft(
            `chen-review-graph-moment chen-review-moment-${moment.tone}`,
            graphStyles.moment,
            momentToneStyles[moment.tone],
            active && graphStyles.momentActive,
            graphStyles.dotFocus,
          )
          const dot = stylex.props(
            graphStyles.dot,
            active && graphStyles.dotActive,
            watercolor && graphStyles.dotWatercolor,
            watercolor && active && graphStyles.dotWatercolorActive,
          )
          return (
            <button
              aria-current={active ? "step" : undefined}
              aria-label={`Evaluation graph: ${moment.label} at ${moment.moveLabel}${evaluation ? `, ${evaluation.label}` : ""}`}
              data-ply={moment.ply}
              disabled={disabled}
              key={moment.ply}
              onClick={() => onSelect(moment.ply)}
              type="button"
              {...button}
              style={{
                ...button.style,
                left: `${hitRegion?.left ?? 0}%`,
                width: `${hitRegion?.width ?? 100}%`,
              }}
            >
              <span
                aria-hidden="true"
                {...dot}
                style={{
                  ...dot.style,
                  left: `${hitRegion?.markerLeft ?? 50}%`,
                  top: `${graphY(evaluation?.value ?? 0)}%`,
                }}
              >
                {moment.glyph}
              </span>
            </button>
          )
        })}
      </div>
    </figure>
  )
}

function graphX(ply: number, maxPly: number) {
  return 7 + (Math.max(0, Math.min(maxPly, ply)) / maxPly) * 86
}

function graphY(value: number) {
  return 50 - (Math.max(-600, Math.min(600, value)) / 600) * 42
}
