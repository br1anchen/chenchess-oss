import type { ComposerState } from "./thread-state"

export function composerConversationBindings(
  composer: ComposerState,
  locked: boolean,
  pendingLabel: string | null = null,
) {
  if (composer.kind === "hostTurn") {
    return {
      busyLabel: composer.progress.label,
      inputDisabled: true,
    }
  }
  return {
    busyLabel: pendingLabel,
    inputDisabled: locked,
  }
}
