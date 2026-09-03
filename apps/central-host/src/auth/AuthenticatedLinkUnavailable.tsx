import {
  HStack,
  Text,
  VStack,
  WatercolorButtonLink,
  WatercolorCard,
} from "@chenchess/ui"

import { coachingBoardDestination } from "./verifiedIdentityDestination"
import { AuthStudio } from "./AuthStudio"
import { SignOutControl } from "./SignOutControl"

/**
 * Owns recovery for a frozen Game Review link that the current account
 * cannot open. The link stays intact while signing out, so the Player can
 * switch accounts and let the verified-identity redirect reopen the original
 * destination instead of being sent to a different resource.
 *
 * Switching accounts is not the only way out, and it used to be the only one
 * offered: a Player who is already on the right account has nothing to switch
 * to, and this card was the end of their session. The Coaching Board link is
 * the recovery for them, and it stays second so the destination-preserving
 * route remains the one read first.
 */
export function AuthenticatedLinkUnavailable({
  account,
  signOut,
}: {
  account: string
  signOut: () => Promise<void>
}) {
  return (
    <AuthStudio legal={false}>
      <WatercolorCard headingLevel={2} title="Link unavailable">
        <VStack gap={3} hAlign="start">
          <Text as="p" display="block" type="body">
            This link is not available on the account signed in as{" "}
            <Text type="body" weight="semibold">
              {account}
            </Text>
            .
          </Text>
          {/* Start-aligned because SignOutControl is a column: it grows a
              status line under its button when logging out fails, and
              centring would slide the Coaching Board link down to meet it. */}
          <HStack gap={2} vAlign="start" wrap="wrap">
            <SignOutControl
              label="Log out and switch account"
              signOut={signOut}
            />
            <WatercolorButtonLink
              href={coachingBoardDestination.href}
              variant="quiet"
            >
              Back to the Coaching Board
            </WatercolorButtonLink>
          </HStack>
        </VStack>
      </WatercolorCard>
    </AuthStudio>
  )
}
