import { useId, useState } from "react"

import {
  Banner,
  Heading,
  languageLayerPrivacyCompanion,
  languageLayerPrivacyHeading,
  Link,
  retentionPreferenceDescription,
  Section,
  Text,
  VStack,
  WatercolorButton,
  WatercolorCheckbox,
  WatercolorField,
  WatercolorInput,
} from "@chenchess/ui"

import { useReviewRetentionPreference } from "./useReviewRetentionPreference"

type FetchAccessToken = (options: {
  forceRefreshToken: boolean
}) => Promise<string | null>

type AccountSettingsProps = {
  fetchAccessToken: FetchAccessToken
  onDeleted: () => Promise<void>
  reauthenticate: (password: string) => Promise<void>
  retention: ReturnType<typeof useReviewRetentionPreference>
}

const accountDeletionConfirmation =
  "DELETE MY CHEN CHESS ACCOUNT IN STAGING AND PRODUCTION"

export function AccountSettings({
  fetchAccessToken,
  onDeleted,
  reauthenticate,
  retention,
}: AccountSettingsProps) {
  const retentionDescriptionId = useId()
  const [deletionConfirmed, setDeletionConfirmed] = useState(false)
  const [deletionFailure, setDeletionFailure] = useState<string | null>(null)
  const [deletionPassword, setDeletionPassword] = useState("")
  const [deletionPending, setDeletionPending] = useState(false)

  async function deleteAccount() {
    if (!deletionConfirmed || !deletionPassword || deletionPending) return
    setDeletionFailure(null)
    setDeletionPending(true)
    try {
      await reauthenticate(deletionPassword)
      const token = await fetchAccessToken({ forceRefreshToken: true })
      if (!token) throw new Error("Firebase session is unavailable")
      const response = await fetch("/api/v1/account/deletion", {
        body: JSON.stringify({
          confirmation: accountDeletionConfirmation,
        }),
        headers: {
          Authorization: `Bearer ${token}`,
          "Content-Type": "application/json",
        },
        method: "POST",
      })
      if (response.status !== 204) {
        throw new Error(
          response.status === 401
            ? "Sign in again, then retry — this needs a recent sign-in."
            : "Deletion could not finish. Your data is still blocked from use; it is safe to retry.",
        )
      }
      await onDeleted()
    } catch (error) {
      setDeletionFailure(
        error instanceof Error ? error.message : "Deletion could not finish.",
      )
    } finally {
      setDeletionPending(false)
    }
  }

  return (
    <VStack data-account-settings="" gap={4} hAlign="stretch">
      {retention.available ? (
        <VStack gap={2} hAlign="start">
          {/* The companion copy is a sibling, so it reaches the control
                through described-by rather than joining its accessible name. */}
          <WatercolorCheckbox
            aria-describedby={retentionDescriptionId}
            checked={retention.enabled}
            disabled={retention.resolving || deletionPending}
            label="Help improve coaching"
            onChange={(event) =>
              void retention.updateEnabled(event.target.checked)
            }
          />
          <Text id={retentionDescriptionId} type="supporting">
            {retentionPreferenceDescription}
          </Text>
        </VStack>
      ) : null}
      {retention.failure ? (
        <Banner
          description={retention.failure}
          status="error"
          title="Preference unavailable"
        />
      ) : null}
      <Section aria-labelledby="hosted-notes-heading" data-hosted-notes="">
        <VStack gap={2} hAlign="start">
          <Heading id="hosted-notes-heading" level={3}>
            {languageLayerPrivacyHeading}
          </Heading>
          <Text as="p" display="block" type="body">
            {languageLayerPrivacyCompanion}
          </Text>
          <Text as="p" display="block" type="body">
            <Link href="/privacy/">Read the privacy page</Link> for the full
            account of where a hosted note goes.
          </Text>
        </VStack>
      </Section>
      <Section aria-labelledby="delete-account-heading">
        <VStack gap={3} hAlign="stretch">
          <Heading id="delete-account-heading" level={3}>
            Delete account
          </Heading>
          <Text as="p" display="block" type="body">
            This permanently deletes your sign-in, all your coaching data on
            both the beta and the live service, any quality copies we kept, and
            every ChatGPT or Claude connection you have authorized.
          </Text>
          <WatercolorField label="Current password">
            <WatercolorInput
              disabled={deletionPending}
              onChange={(event) => setDeletionPassword(event.target.value)}
              type="password"
              value={deletionPassword}
            />
          </WatercolorField>
          <WatercolorCheckbox
            checked={deletionConfirmed}
            disabled={deletionPending}
            label="I understand this deletes my account in both staging and production."
            onChange={(event) => setDeletionConfirmed(event.target.checked)}
          />
          {deletionFailure ? (
            <Banner
              description={deletionFailure}
              status="error"
              title="Deletion unavailable"
            />
          ) : null}
          <WatercolorButton
            block
            disabled={
              deletionPending ||
              !deletionConfirmed ||
              deletionPassword.length === 0
            }
            loading={deletionPending}
            onClick={() => void deleteAccount()}
            type="button"
            variant="danger"
          >
            {deletionPending
              ? "Deleting account…"
              : "Permanently delete account"}
          </WatercolorButton>
        </VStack>
      </Section>
    </VStack>
  )
}
