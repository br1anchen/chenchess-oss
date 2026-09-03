import {
  Heading,
  Text,
  WatercolorButtonLink,
  WatercolorCard,
} from "@chenchess/ui"

import { PublicUtilityPage } from "./PublicUtilityPage"

export function SupportPage() {
  return (
    <PublicUtilityPage current="support">
      <WatercolorCard padding="comfortable" tone="bamboo">
        <Heading level={1}>Support</Heading>
        <Text as="p" display="block">
          This instance is self-hosted, so support comes from whoever runs it.
          For the software itself, the source and its documentation are the
          first place to look.
        </Text>
        <WatercolorButtonLink
          href="https://github.com/br1anchen/chenchess-oss"
          size="lg"
        >
          Read the source
        </WatercolorButtonLink>
      </WatercolorCard>
      <WatercolorCard padding="comfortable" tone="paper">
        <Heading level={2}>Keep your account safe</Heading>
        <Text as="p" display="block">
          Never send passwords, tokens, or invitation codes by email. For data
          or privacy requests, include only the details needed to find the
          account.
        </Text>
      </WatercolorCard>
    </PublicUtilityPage>
  )
}
