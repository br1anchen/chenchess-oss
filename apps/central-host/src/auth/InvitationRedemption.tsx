import { useState, type FormEvent } from "react"

import {
  VStack,
  WatercolorButton,
  WatercolorField,
  WatercolorInput,
} from "@chenchess/ui"

import { AuthNotice, type AuthNoticeStatus } from "./AuthNotice"
import type { FetchAccessToken } from "./FirebaseAuthProvider"
import {
  redeemInvitation,
  type InvitationRedemptionResult,
} from "./redeemInvitation"

export const invitationRedemptionCopy = {
  granted: "Access granted to this account.",
  wrongAccount:
    "This invite belongs to a different email address. Sign in with the invited account.",
  verificationRequired: "Verify the invited email address, then try again.",
  revoked: "This invitation is no longer active.",
  invalid: "That invitation code is not valid. Check the code and try again.",
  alreadyHandled: "This invitation has already been handled.",
  tryLater: "Too many attempts. Please wait before trying again.",
  unavailable:
    "Invitation redemption is temporarily unavailable. Please try again.",
} as const satisfies Record<InvitationRedemptionResult["kind"], string>

export function InvitationRedemption({
  fetchAccessToken,
  initialCode,
  onRedeemed,
}: {
  fetchAccessToken: FetchAccessToken
  initialCode: string | null
  onRedeemed: () => void
}) {
  const [code, setCode] = useState(initialCode ?? "")
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [result, setResult] = useState<InvitationRedemptionResult | null>(null)

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setResult(null)
    setIsSubmitting(true)
    const redemption = await redeemInvitation(fetchAccessToken, code.trim())
    setResult(redemption)
    if (redemption.kind === "granted") {
      setCode("")
      onRedeemed()
    }
    setIsSubmitting(false)
  }

  return (
    <form onSubmit={(event) => void submit(event)}>
      <VStack gap={3} hAlign="stretch">
        <WatercolorField
          hint={
            initialCode
              ? "Invitation link captured securely."
              : "Paste the code from your invitation email to redeem on this device."
          }
          label="Invitation code"
        >
          <WatercolorInput
            name="invitationCode"
            onChange={(event) => setCode(event.target.value)}
            value={code}
          />
        </WatercolorField>
        {result ? (
          <AuthNotice
            message={redemptionMessage(result)}
            status={redemptionStatus(result)}
          />
        ) : null}
        <WatercolorButton
          block
          disabled={isSubmitting || code.trim().length === 0}
          loading={isSubmitting}
          type="submit"
          variant="primary"
        >
          {isSubmitting ? "Redeeming…" : "Redeem invitation"}
        </WatercolorButton>
      </VStack>
    </form>
  )
}

function redemptionMessage(result: InvitationRedemptionResult): string {
  return invitationRedemptionCopy[result.kind]
}

function redemptionStatus(
  result: InvitationRedemptionResult,
): AuthNoticeStatus {
  const { kind } = result
  switch (kind) {
    case "granted":
      return "success"
    case "alreadyHandled":
      return "info"
    case "verificationRequired":
    case "tryLater":
      return "warning"
    case "wrongAccount":
    case "revoked":
    case "invalid":
    case "unavailable":
      return "error"
    default: {
      const _exhaustive: never = kind
      return _exhaustive
    }
  }
}
