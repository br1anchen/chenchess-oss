import * as stylex from "@stylexjs/stylex"

import { WatercolorButton } from "../components/watercolor"
import { Icon } from "../icons"
import { navigatorStyles } from "./ReviewNavigation.styles"
import {
  reviewMomentCountLabel,
  type ReviewContextNavigationProps,
  type ReviewMomentMarkerPresentation,
} from "./reviewNavigationPresentation"

export type ReviewMomentNavigatorProps = ReviewContextNavigationProps & {
  ariaLabel?: string
  discussDisabled?: boolean
  discussLabel?: string
  discussing?: boolean
  onDiscuss?: () => void
  title?: string
}

/**
 * A compact, single-row Critical Moment selector. The evaluation graph remains
 * the visual overview; this control only identifies the active moment and
 * steps chronologically through the prepared set.
 */
export function ReviewMomentNavigator({
  activePly,
  ariaLabel = "Critical moment navigation",
  discussDisabled,
  discussLabel = "Discuss in chat",
  discussing = false,
  disabled,
  moments,
  onDiscuss,
  onSelect,
  title = "Critical moments",
}: ReviewMomentNavigatorProps) {
  const activeIndex = Math.max(
    0,
    moments.findIndex((moment) => moment.ply === activePly),
  )
  const activeMoment = moments[activeIndex]
  if (!activeMoment) return null
  const countLabel = reviewMomentCountLabel(moments, activeMoment.ply)

  return (
    <section
      aria-label={ariaLabel}
      data-has-discuss={onDiscuss ? "true" : undefined}
      {...craft(
        "chen-review-moment-navigator",
        navigatorStyles.navigator,
        Boolean(onDiscuss) && navigatorStyles.navigatorWithDiscuss,
      )}
    >
      <WatercolorButton
        aria-label="Previous critical moment"
        disabled={disabled || activeIndex === 0}
        onClick={() => onSelect(moments[activeIndex - 1]!.ply)}
        size="icon"
        type="button"
        variant="quiet"
        xstyle={navigatorStyles.stepButton}
      >
        <Icon icon="chevronLeft" size="sm" xstyle={navigatorStyles.stepIcon} />
      </WatercolorButton>
      <MomentIdentity moment={activeMoment} />
      <h2
        aria-label={`${title} ${countLabel}`}
        {...stylex.props(navigatorStyles.title)}
      >
        <span>{title}</span>
        <output
          aria-label={`${countLabel}: ${activeMoment.moveLabel}`}
          aria-live="polite"
          {...stylex.props(navigatorStyles.count)}
        >
          {countLabel}
        </output>
      </h2>
      <WatercolorButton
        aria-label="Next critical moment"
        disabled={disabled || activeIndex === moments.length - 1}
        onClick={() => onSelect(moments[activeIndex + 1]!.ply)}
        size="icon"
        type="button"
        variant="quiet"
        xstyle={navigatorStyles.stepButton}
      >
        <Icon icon="chevronRight" size="sm" xstyle={navigatorStyles.stepIcon} />
      </WatercolorButton>
      {onDiscuss ? (
        <WatercolorButton
          aria-label={discussLabel}
          className="chen-review-moment-discuss"
          disabled={disabled || discussDisabled}
          loading={discussing}
          onClick={onDiscuss}
          size="sm"
          type="button"
          xstyle={navigatorStyles.discuss}
        >
          <Icon icon="messageCircle" size="sm" />
          <span>{discussLabel}</span>
        </WatercolorButton>
      ) : null}
    </section>
  )
}

function MomentIdentity({
  moment,
}: {
  moment: ReviewMomentMarkerPresentation
}) {
  return (
    <div {...craft("chen-review-moment-identity", navigatorStyles.identity)}>
      <strong {...stylex.props(navigatorStyles.moveLabel)}>
        {moment.moveLabel}
      </strong>
      <span {...stylex.props(navigatorStyles.label)}>{moment.label}</span>
      {moment.summary ? (
        <small {...stylex.props(navigatorStyles.detail)}>
          {moment.summary}
        </small>
      ) : null}
    </div>
  )
}

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
