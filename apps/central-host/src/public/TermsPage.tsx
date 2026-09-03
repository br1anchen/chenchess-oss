import { Heading, Text, WatercolorCard } from "@chenchess/ui"

import { PublicUtilityPage } from "./PublicUtilityPage"

export function TermsPage() {
  return (
    <PublicUtilityPage current="terms">
      <WatercolorCard padding="comfortable" tone="mist">
        <Heading level={1}>Terms</Heading>
        <Text as="p" display="block">
          This software is distributed under the GNU Affero General Public
          License v3.0 or later, and comes with no warranty. The licence text
          governs; whoever runs this instance sets any terms of their own.
        </Text>
      </WatercolorCard>
      <WatercolorCard padding="comfortable" tone="paper">
        <Heading level={2}>What it is for</Heading>
        <Text as="p" display="block">
          Review completed games. The coaching is educational: every claim is
          traced to engine evidence, and none of it is a guarantee about how a
          position should be played.
        </Text>
      </WatercolorCard>
    </PublicUtilityPage>
  )
}
