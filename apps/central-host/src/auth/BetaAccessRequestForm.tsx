import { useState, type FormEvent } from "react"

import {
  Text,
  VStack,
  WatercolorButton,
  WatercolorCard,
  WatercolorField,
  WatercolorInput,
} from "@chenchess/ui"

import { AuthNotice, type AuthNoticeStatus } from "./AuthNotice"
import type { FetchAccessToken } from "./FirebaseAuthProvider"
import {
  requestBetaAccess,
  type BetaAccessRequestResult,
} from "./requestBetaAccess"

export function BetaAccessRequestForm({
  email,
  fetchAccessToken,
}: {
  email: string | null
  fetchAccessToken: FetchAccessToken
}) {
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [result, setResult] = useState<BetaAccessRequestResult | null>(null)

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!email) return
    setResult(null)
    setIsSubmitting(true)
    setResult(await requestBetaAccess({ fetchAccessToken }))
    setIsSubmitting(false)
  }

  if (!email) {
    return (
      <WatercolorCard headingLevel={2} title="Ask for an invite">
        <Text as="p" display="block" type="supporting">
          This account has no email address. Sign out and sign in with email and
          password, or with Google.
        </Text>
      </WatercolorCard>
    )
  }

  return (
    <WatercolorCard headingLevel={2} title="Ask for an invite">
      <form onSubmit={(event) => void submit(event)}>
        <VStack gap={3} hAlign="stretch">
          <WatercolorField
            hint="Asking more than once is fine — you get the same confirmation."
            label="Verified email"
          >
            <WatercolorInput name="email" readOnly type="email" value={email} />
          </WatercolorField>
          {result ? (
            <AuthNotice
              message={result.message}
              status={requestStatus(result)}
            />
          ) : null}
          <WatercolorButton
            block
            disabled={isSubmitting}
            loading={isSubmitting}
            type="submit"
            variant="primary"
          >
            {isSubmitting ? "Sending…" : "Ask for an invite"}
          </WatercolorButton>
        </VStack>
      </form>
    </WatercolorCard>
  )
}

function requestStatus(result: BetaAccessRequestResult): AuthNoticeStatus {
  const { kind } = result
  switch (kind) {
    case "accepted":
      return "success"
    case "session":
      return "warning"
    case "unavailable":
      return "error"
    default: {
      const _exhaustive: never = kind
      return _exhaustive
    }
  }
}
