import {
  Heading,
  languageLayerPrivacyHeading,
  languageLayerPrivacyParagraphs,
  Text,
  VStack,
  WatercolorCard,
} from "@chenchess/ui"

import { publicUtilityStyles } from "./publicUtility.styles"
import { PublicUtilityPage } from "./PublicUtilityPage"

export function PrivacyPage() {
  return (
    <PublicUtilityPage current="privacy">
      <WatercolorCard padding="comfortable" tone="mist">
        <Heading level={1}>Privacy</Heading>
        <Text as="p" display="block">
          This page explains where your coaching words go. It describes the
          software, not any one deployment: whoever runs this instance decides
          what it is configured to talk to.
        </Text>
      </WatercolorCard>
      <WatercolorCard padding="comfortable" tone="paper">
        <Heading level={2}>{languageLayerPrivacyHeading}</Heading>
        <VStack hAlign="start" xstyle={publicUtilityStyles.articleCopy}>
          <Text as="p" display="block">
            An instance with no model provider configured sends nothing
            anywhere: the coach writes every note itself from the engine&apos;s
            own analysis. That is the default, and it is what a local install
            does until someone configures otherwise. The rest of this section
            describes what happens once a provider is configured.
          </Text>
          {/* Rendered from the shared constant rather than copied: this is a
              governed claim, and the page must not be able to drift from it. */}
          {languageLayerPrivacyParagraphs.map((paragraph) => (
            <Text as="p" display="block" key={paragraph}>
              {paragraph}
            </Text>
          ))}
        </VStack>
      </WatercolorCard>
      <WatercolorCard padding="comfortable" tone="paper">
        <Heading level={2}>Questions or requests</Heading>
        <Text as="p" display="block">
          Privacy and data requests go to whoever runs this instance. If you are
          running it yourself, that is you, and the data is in your own
          Firestore. Never send passwords, tokens, or invitation codes by email.
        </Text>
      </WatercolorCard>
    </PublicUtilityPage>
  )
}
