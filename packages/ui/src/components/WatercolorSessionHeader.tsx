import { HStack } from "@astryxdesign/core/HStack"
import { VStack } from "@astryxdesign/core/VStack"
import * as stylex from "@stylexjs/stylex"
import type { ReactNode } from "react"

import { BrandLockup } from "./BrandLockup"
import { WatercolorEyebrow, WatercolorPlaque } from "./watercolor"
import { cardPartStyles, sessionHeaderStyles } from "./watercolor.styles"

export type WatercolorSessionHeaderProps = {
  actions?: ReactNode
  eyebrow?: string
  /** Context beside the plaque: on a desktop line it sits next to the title,
   * on a phone it wraps underneath so the header stays compact. */
  meta?: ReactNode
  title?: ReactNode
}

/**
 * Session page header. The title keeps a 12ch floor so a nowrap badge or
 * action cluster cannot crush it to 0px (R5). Lives outside watercolor.tsx
 * so Coach App widgets do not pull `brandAssets` through BrandLockup.
 *
 * One flat wrapping row, reordered by breakpoint: on a desktop line it reads
 * brand · title · meta … actions; on a phone flex order puts the actions
 * beside the brand and gives the plaque and meta full rows of their own.
 */
export function WatercolorSessionHeader({
  actions,
  eyebrow,
  meta,
  title,
}: WatercolorSessionHeaderProps) {
  return (
    <HStack
      as="header"
      className="chen-watercolor-session-header"
      gap={3}
      vAlign="center"
      wrap="wrap"
      xstyle={sessionHeaderStyles.row}
    >
      <BrandLockup size="workspace" />
      {title || eyebrow ? (
        <VStack gap={1} hAlign="start" xstyle={sessionHeaderStyles.title}>
          {title ? (
            <>
              {eyebrow ? (
                <WatercolorEyebrow>{eyebrow}</WatercolorEyebrow>
              ) : null}
              <h1
                {...craft(
                  ["chen-watercolor-session-title"],
                  sessionHeaderStyles.heading,
                )}
              >
                {title}
              </h1>
            </>
          ) : (
            <h1
              {...craft(
                ["chen-watercolor-session-title"],
                sessionHeaderStyles.plaqueHeading,
              )}
            >
              <WatercolorPlaque
                className="chen-watercolor-session-subtitle"
                size="lg"
                xstyle={[
                  cardPartStyles.titlePlaque,
                  sessionHeaderStyles.plaqueStretch,
                ]}
              >
                {eyebrow}
              </WatercolorPlaque>
            </h1>
          )}
        </VStack>
      ) : null}
      {meta ? (
        <VStack
          className="chen-watercolor-session-meta"
          gap={0}
          hAlign="start"
          xstyle={sessionHeaderStyles.meta}
        >
          {meta}
        </VStack>
      ) : null}
      {actions ? (
        <HStack gap={2} wrap="wrap" xstyle={sessionHeaderStyles.actions}>
          {actions}
        </HStack>
      ) : null}
    </HStack>
  )
}

function craft(
  classNames: ReadonlyArray<string | false | undefined | null>,
  ...styles: ReadonlyArray<object | false | undefined | null>
) {
  // SAFETY: every argument comes from `stylex.create` in watercolor.styles.ts;
  // the compiler validated the declarations, the published parameter type just
  // cannot express them (conditions inside pseudo-element blocks).
  const applyStyles = stylex.props as (
    ...applied: readonly unknown[]
  ) => ReturnType<typeof stylex.props>
  const sx = applyStyles(...styles.filter(Boolean))
  return {
    ...sx,
    className: [sx.className, ...classNames].filter(Boolean).join(" "),
  }
}
