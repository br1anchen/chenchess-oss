import type { ReactNode } from "react"

import { Icon } from "@chenchess/ui/astryx"
import {
  BrandLockup,
  HStack,
  SessionHeaderLabel,
  WatercolorButton,
} from "@chenchess/ui"
import * as stylex from "@stylexjs/stylex"

import { SignOutControl } from "@/auth/SignOutControl"

const styles = stylex.create({
  header: {
    width: "100%",
  },
})

/**
 * The header every authenticated surface wears: the wordmark, and the two
 * controls that belong to the account rather than to the board.
 *
 * There is no surface above the Coaching Board to return to, so this carries
 * no navigation of its own.
 */
export function AppHeader({
  extra,
  heading,
  onAccountSettings,
  signOut,
}: {
  extra?: ReactNode
  heading: string
  onAccountSettings?: () => void
  signOut?: () => Promise<void>
}) {
  return (
    <HStack
      as="header"
      gap={3}
      hAlign="between"
      vAlign="center"
      wrap="wrap"
      xstyle={styles.header}
    >
      <h1 className="sr-only">{heading}</h1>
      <HStack gap={3} vAlign="center" wrap="wrap">
        <BrandLockup href="/" size="header" />
      </HStack>
      <HStack gap={2} vAlign="center" wrap="wrap">
        {onAccountSettings ? (
          <WatercolorButton
            aria-label="Account settings"
            onClick={onAccountSettings}
            size="sm"
            type="button"
            variant="quiet"
          >
            <Icon icon="settings" size="sm" />
            <SessionHeaderLabel>Account settings</SessionHeaderLabel>
          </WatercolorButton>
        ) : null}
        {signOut ? (
          <SignOutControl signOut={signOut} size="sm" variant="quiet" />
        ) : null}
        {extra}
      </HStack>
    </HStack>
  )
}
