import { Icon } from "@chenchess/ui/astryx"
import { useState } from "react"

import { Button, HStack, Text } from "@chenchess/ui"

/**
 * The Player's way to say "this" to a coach in another window (#530).
 *
 * The chat is not on the page, and the burden of putting the board into
 * words has been the Player's alone. One press copies a referent that names
 * what the Player is looking at, ready to paste in front of the question.
 * Clipboard only — WebMCP has no channel into the chat input, and the
 * referent is prose the Player sends, not a call the agent makes. The coach
 * reads the board anyway; the sentence only has to make it look.
 *
 * A clipboard the page cannot write — an embedded browser that denies the
 * permission — is a real place this runs, so the sentence is shown instead
 * for the Player to copy by hand rather than the press doing nothing.
 */
export function AskTheCoach({
  copyReferent,
  label,
  referent,
}: {
  copyReferent: (referent: string) => Promise<void>
  label: string
  referent: string
}) {
  const [outcome, setOutcome] = useState<"blocked" | "copied" | "idle">("idle")
  async function copy() {
    try {
      await copyReferent(referent)
      setOutcome("copied")
    } catch {
      setOutcome("blocked")
    }
  }
  return (
    <HStack
      aria-label="Ask the coach"
      gap={2}
      role="group"
      vAlign="center"
      wrap="wrap"
    >
      <Button
        clickAction={copy}
        icon={<Icon icon="messageCircle" size="sm" />}
        label={label}
        size="sm"
        type="button"
        variant="secondary"
      />
      <Text role="status" type="supporting">
        {copyOutcomeInWords(outcome, referent)}
      </Text>
    </HStack>
  )
}

function copyOutcomeInWords(
  outcome: "blocked" | "copied" | "idle",
  referent: string,
) {
  switch (outcome) {
    case "idle":
      return ""
    case "copied":
      return "Copied. Paste it into the chat, then ask."
    case "blocked":
      return `Copying is blocked here. Paste this into the chat: ${referent}`
  }
}
