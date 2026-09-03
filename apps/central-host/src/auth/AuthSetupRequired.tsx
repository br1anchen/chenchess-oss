import { Text, VStack, WatercolorCard } from "@chenchess/ui"

import { AuthNotice } from "./AuthNotice"
import { AuthStudio } from "./AuthStudio"

export function AuthSetupRequired() {
  return (
    <AuthStudio>
      <WatercolorCard
        headingLevel={2}
        title="Firebase Authentication is not configured"
      >
        <VStack gap={3} hAlign="start">
          <Text as="p" display="block" type="body">
            Add the public Firebase web application settings before opening the
            authenticated web product.
          </Text>
          <AuthNotice
            detail="Set `VITE_FIREBASE_API_KEY`, `VITE_FIREBASE_AUTH_DOMAIN`, `VITE_FIREBASE_PROJECT_ID`, and `VITE_FIREBASE_APP_ID` in `apps/central-host/.env.local`, then restart the Central Host."
            message="Required for auth"
            status="warning"
          />
        </VStack>
      </WatercolorCard>
    </AuthStudio>
  )
}
