import type { ReactNode } from "react"

import {
  BrandLockup,
  HStack,
  Text,
  VStack,
  WatercolorStudio,
} from "@chenchess/ui"

import { authStudioStyles } from "./authStudio.styles"
import { PublicLegalLinks } from "./PublicLegalLinks"

export function AuthStudio({
  children,
  eyebrow,
  legal = true,
  wide = false,
}: {
  children: ReactNode
  eyebrow?: string
  legal?: boolean
  wide?: boolean
}) {
  return (
    <WatercolorStudio as="main" xstyle={authStudioStyles.page}>
      <VStack
        gap={4}
        hAlign="stretch"
        maxWidth={wide ? "48rem" : "28rem"}
        xstyle={authStudioStyles.column}
      >
        <HStack as="header" gap={3} vAlign="center">
          <BrandLockup href="/" size="header" />
          {eyebrow ? (
            <Text color="secondary" type="label">
              {eyebrow}
            </Text>
          ) : null}
        </HStack>
        {children}
        {legal ? <PublicLegalLinks /> : null}
      </VStack>
    </WatercolorStudio>
  )
}
