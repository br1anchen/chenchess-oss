import * as stylex from "@stylexjs/stylex"
import { useEffect, useRef, type ReactNode } from "react"
import { Heading } from "../astryx"
import { Icon } from "../icons"
import {
  WatercolorButton,
  WatercolorMomentCard,
} from "../components/watercolor"
import { navButtonStyles, pickerStyles } from "./ReviewNavigation.styles"
import {
  reviewMomentCountLabel,
  type ReviewContextNavigationProps,
  type ReviewMomentMarkerPresentation,
} from "./reviewNavigationPresentation"

export type ReviewMomentSlideState = {
  /** This moment is the snapped, selected one. */
  active: boolean
}

export type ReviewMomentCarouselProps = ReviewContextNavigationProps & {
  ariaLabel?: string
  /** `compact` is the selector widget, where the picker shares a host
   * viewport with the board it drives. */
  density?: "default" | "compact"
  /**
   * Body rendered inside each slide, below the moment card. Supplying one turns
   * the carousel into a compound card: the board, legend, and call to action
   * travel with the moment as it is swiped. Omit it for a bare moment picker.
   *
   * Only the active slide and its immediate neighbours call this, since they
   * are the only ones a swipe can reveal. Bodies are free to be expensive.
   */
  renderMoment?: (
    moment: ReviewMomentMarkerPresentation,
    state: ReviewMomentSlideState,
  ) => ReactNode
  /**
   * Context rendered opposite the title — the game's opening and Elo stamps.
   * Describes the review the moments belong to, not the moment selected.
   */
  headerExtra?: ReactNode
  /**
   * Heading rendered inside the framed selector card, centered with the
   * moment, with the live count beside it. Defaults to “Critical moments”.
   */
  title?: string
  /** Host sizing for the picker card — compiled StyleX only. */
  xstyle?: object
}

/** How many slides either side of the active one mount their body. */
const reachableSlides = 1

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

export function ReviewMomentCarousel({
  activePly,
  ariaLabel = "Review moments",
  density = "default",
  disabled,
  headerExtra,
  moments,
  onSelect,
  renderMoment,
  title,
  xstyle,
}: ReviewMomentCarouselProps) {
  const optionsRef = useRef<HTMLDivElement>(null)
  const scrollStopRef = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  )
  const activeIndex = Math.max(
    0,
    moments.findIndex((moment) => moment.ply === activePly),
  )
  const activeMoment = moments[activeIndex]
  const activeMomentPly = activeMoment?.ply
  const compact = density === "compact"

  useEffect(() => {
    if (activeMomentPly === undefined) return
    const options = optionsRef.current
    const activeSlide = options?.querySelector<HTMLElement>(
      `[data-slide-ply="${activeMomentPly}"]`,
    )
    if (!options || !activeSlide) return
    const left = Math.max(0, activeSlide.offsetLeft)
    if (parseHasScrollTo(options.scrollTo)) {
      options.scrollTo({ behavior: "auto", left })
    } else {
      options.scrollLeft = left
    }
  }, [activeMomentPly])

  useEffect(
    () => () => {
      if (scrollStopRef.current) clearTimeout(scrollStopRef.current)
    },
    [],
  )

  if (!activeMoment) return null

  const countLabel = reviewMomentCountLabel(moments, activeMoment.ply)
  const headingTitle = title ?? "Critical moments"
  const headingCentered = headerExtra === undefined

  const countOutput = (
    <output
      aria-label={`${countLabel}: ${activeMoment.moveLabel}`}
      aria-live="polite"
      {...craft("chen-review-moment-count", pickerStyles.count)}
    >
      {countLabel}
    </output>
  )

  return (
    <section
      aria-label={ariaLabel}
      aria-roledescription="carousel"
      data-compound={renderMoment ? "true" : undefined}
      data-watercolor-surface="moment-carousel"
      {...craft(
        "chen-review-moment-picker",
        pickerStyles.picker,
        compact && pickerStyles.pickerCompact,
        xstyle,
      )}
    >
      <div
        {...craft(
          "chen-review-moment-header",
          pickerStyles.header,
          headingCentered && pickerStyles.headerCentered,
        )}
      >
        <Heading
          aria-label={`${headingTitle} ${countLabel}`}
          level={2}
          xstyle={[
            pickerStyles.title,
            headingCentered && pickerStyles.titleCentered,
          ]}
        >
          {headingTitle}
          {countOutput}
        </Heading>
        {headerExtra}
      </div>
      <div
        aria-label="Critical moment cards"
        onScroll={(event) => {
          if (disabled) return
          if (scrollStopRef.current) clearTimeout(scrollStopRef.current)
          const options = event.currentTarget
          scrollStopRef.current = setTimeout(() => {
            const viewportCenter = options.scrollLeft + options.clientWidth / 2
            const nearest = [
              ...options.querySelectorAll<HTMLElement>(
                ".chen-review-moment-slide[data-slide-ply]",
              ),
            ].reduce<HTMLElement | undefined>((closest, candidate) => {
              if (!closest) return candidate
              const candidateDistance = Math.abs(
                candidate.offsetLeft +
                  candidate.offsetWidth / 2 -
                  viewportCenter,
              )
              const closestDistance = Math.abs(
                closest.offsetLeft + closest.offsetWidth / 2 - viewportCenter,
              )
              return candidateDistance < closestDistance ? candidate : closest
            }, undefined)
            const ply = Number(nearest?.dataset.slidePly)
            if (Number.isFinite(ply) && ply !== activeMomentPly) onSelect(ply)
          }, 120)
        }}
        ref={optionsRef}
        role="group"
        {...craft("chen-review-moment-options", pickerStyles.options)}
      >
        {moments.map((moment, index) => {
          const previous = moments[index - 1]
          const active = moment.ply === activePly
          const reachable = Math.abs(index - activeIndex) <= reachableSlides
          return (
            <div
              aria-label={`${index + 1} of ${moments.length}`}
              aria-roledescription="slide"
              data-slide-ply={moment.ply}
              key={moment.ply}
              role="group"
              {...craft(
                "chen-review-moment-slide",
                pickerStyles.slide,
                compact && pickerStyles.slideCompact,
              )}
            >
              <div
                aria-label={
                  active ? "Critical moment carousel controls" : undefined
                }
                role={active ? "group" : undefined}
                {...craft(
                  "chen-review-moment-row",
                  pickerStyles.row,
                  compact && pickerStyles.rowCompact,
                )}
              >
                {active ? (
                  <WatercolorButton
                    aria-label="Previous critical moment"
                    className="chen-review-moment-nav-button"
                    hoverWash="none"
                    disabled={disabled || activeIndex === 0}
                    onClick={() => onSelect(moments[activeIndex - 1]!.ply)}
                    size="icon"
                    type="button"
                    variant="secondary"
                    xstyle={[
                      navButtonStyles.base,
                      navButtonStyles.previous,
                      compact && navButtonStyles.compact,
                    ]}
                  >
                    <Icon
                      icon="chevronLeft"
                      size="sm"
                      xstyle={navButtonStyles.icon}
                    />
                  </WatercolorButton>
                ) : (
                  <span aria-hidden="true" />
                )}
                <WatercolorMomentCard
                  cardXstyle={
                    compact ? pickerStyles.momentCardCompact : undefined
                  }
                  current={active}
                  data-ply={moment.ply}
                  detail={
                    moment.summary ??
                    (previous
                      ? `Since ${previous.moveLabel}`
                      : "First review moment")
                  }
                  disabled={disabled}
                  density={density}
                  glyph={moment.glyph}
                  label={moment.label}
                  moveLabel={moment.moveLabel}
                  onClick={() => onSelect(moment.ply)}
                  tone={moment.tone}
                  type="button"
                />
                {active ? (
                  <WatercolorButton
                    aria-label="Next critical moment"
                    className="chen-review-moment-nav-button"
                    hoverWash="none"
                    disabled={disabled || activeIndex === moments.length - 1}
                    onClick={() => onSelect(moments[activeIndex + 1]!.ply)}
                    size="icon"
                    type="button"
                    variant="secondary"
                    xstyle={[
                      navButtonStyles.base,
                      navButtonStyles.next,
                      compact && navButtonStyles.compact,
                    ]}
                  >
                    <Icon
                      icon="chevronRight"
                      size="sm"
                      xstyle={navButtonStyles.icon}
                    />
                  </WatercolorButton>
                ) : (
                  <span aria-hidden="true" />
                )}
              </div>
              {renderMoment ? (
                // Only the snapped slide's body is announced. The neighbours
                // exist so a swipe has something to reveal, but their boards
                // and actions would otherwise duplicate the current one in the
                // accessibility tree.
                <div
                  aria-hidden={active ? undefined : "true"}
                  data-slide-body=""
                  {...craft(
                    "chen-review-moment-body",
                    pickerStyles.body,
                    compact && pickerStyles.bodyCompact,
                  )}
                >
                  {reachable ? renderMoment(moment, { active }) : null}
                </div>
              ) : null}
            </div>
          )
        })}
      </div>
    </section>
  )
}

function parseHasScrollTo(
  value: unknown,
): value is (options: ScrollToOptions) => void {
  return typeof value === "function"
}
