import { useState } from "react"

import { Icon } from "@chenchess/ui/astryx"
import {
  SessionHeaderLabel,
  Text,
  VStack,
  WatercolorButton,
} from "@chenchess/ui"

export function SignOutControl({
  label = "Log out",
  signOut,
  size = "md",
  variant = "secondary",
}: {
  label?: string
  signOut: () => Promise<void>
  size?: "md" | "sm"
  variant?: "quiet" | "secondary"
}) {
  const [state, setState] = useState<"idle" | "submitting" | "unavailable">(
    "idle",
  )

  async function submit() {
    setState("submitting")
    try {
      await signOut()
    } catch {
      setState("unavailable")
    }
  }

  return (
    <VStack gap={2} hAlign="start">
      <WatercolorButton
        aria-label={label}
        disabled={state === "submitting"}
        loading={state === "submitting"}
        onClick={() => void submit()}
        size={size}
        type="button"
        variant={variant}
      >
        <Icon icon="logOut" size="sm" />
        <SessionHeaderLabel>
          {state === "submitting" ? "Logging out…" : label}
        </SessionHeaderLabel>
      </WatercolorButton>
      {state === "unavailable" ? (
        <Text role="status" type="supporting">
          Log out is temporarily unavailable.
        </Text>
      ) : null}
    </VStack>
  )
}
