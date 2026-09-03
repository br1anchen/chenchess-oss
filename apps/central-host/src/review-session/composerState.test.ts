import { expect, test } from "vitest"

import { composerConversationBindings } from "./composerState"
import { hostTurnStepLabels } from "./thread-state"

test("hostTurn composer locks input and shows the D9 label", () => {
  expect(
    composerConversationBindings(
      {
        kind: "hostTurn",
        draft: "",
        progress: { label: hostTurnStepLabels.writing },
      },
      false,
      "Opening…",
    ),
  ).toEqual({
    busyLabel: hostTurnStepLabels.writing,
    inputDisabled: true,
  })
})

test("idle composer uses the pending navigation label", () => {
  expect(
    composerConversationBindings({ kind: "idle", draft: "" }, true, "Opening…"),
  ).toEqual({
    busyLabel: "Opening…",
    inputDisabled: true,
  })
})
