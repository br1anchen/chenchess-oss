import type { WorkerMessage } from "./upstream-browser-harness-protocol"

export const workerStdinClosedInFlightMessage =
  "The MCPJam worker stdin closed while a command was in flight."

export function createInFlightCommandGate(
  send: (message: WorkerMessage) => void,
) {
  let id: string | null = null
  let interrupted = false

  return {
    begin(commandId: string): void {
      id = commandId
    },
    end(): void {
      if (!interrupted) {
        id = null
      }
    },
    get interrupted(): boolean {
      return interrupted
    },
    interruptStdinClosed(): boolean {
      if (id !== null) {
        send({
          id,
          kind: "response",
          outcome: {
            kind: "error",
            message: workerStdinClosedInFlightMessage,
          },
        })
        id = null
        interrupted = true
      }
      return interrupted
    },
  }
}

export type DisposeAndExitDecision = {
  readonly exitCode: 0 | 1
  readonly shouldCallProcessExitZero: boolean
}

export function disposeAndExitDecision(
  commandWasInFlight: boolean,
  existingExitCode: string | number | null | undefined = null,
): DisposeAndExitDecision {
  if (
    existingExitCode === 1 ||
    existingExitCode === "1" ||
    commandWasInFlight
  ) {
    return { exitCode: 1, shouldCallProcessExitZero: false }
  }
  return { exitCode: 0, shouldCallProcessExitZero: true }
}
