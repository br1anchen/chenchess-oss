import {
  Heading,
  Text,
  VStack,
  WatercolorButtonLink,
  WatercolorCard,
} from "@chenchess/ui"

import { notFoundStyles } from "./publicUtility.styles"
import { PublicUtilityPage } from "./PublicUtilityPage"

export function NotFoundPage() {
  return (
    <PublicUtilityPage>
      <WatercolorCard padding="comfortable" tone="mist">
        <Heading level={1}>Page not found</Heading>
        <VStack hAlign="start" xstyle={notFoundStyles.actions}>
          <Text as="p" display="block" xstyle={notFoundStyles.note}>
            That address is not a ChenChess page. The landing, privacy, terms,
            and support pages are still here.
          </Text>
          <WatercolorButtonLink href="/" size="lg">
            Back to ChenChess
          </WatercolorButtonLink>
        </VStack>
      </WatercolorCard>
    </PublicUtilityPage>
  )
}
