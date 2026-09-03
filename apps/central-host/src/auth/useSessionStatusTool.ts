import { useEffect, useRef } from "react"

import {
  readSessionStatusDescription,
  sessionStatusResult,
  type SessionStatus,
} from "./sessionStatus"

const emptyInputSchema = {
  additionalProperties: false,
  properties: {},
  type: "object",
} as const

/**
 * Register the session-status read on the Sign-In Page or Beta Admission Page.
 *
 * Called without an authorized Player. Register only when the page is staying
 * on a locked stage. Redirect shells pass null and register nothing.
 */
export function useSessionStatusTool(status: SessionStatus | null) {
  const statusRef = useRef(status)

  useEffect(() => {
    statusRef.current = status
  }, [status])

  useEffect(() => {
    const registered = statusRef.current
    if (!registered) return
    const modelContext = document.modelContext
    if (!modelContext) return
    const controller = new AbortController()
    modelContext.registerTool(
      {
        annotations: { idempotentHint: true, readOnlyHint: true },
        description: readSessionStatusDescription,
        execute: () => ({
          structuredContent: sessionStatusResult(
            statusRef.current ?? registered,
          ),
        }),
        inputSchema: emptyInputSchema,
        name: "read_session_status",
      },
      { signal: controller.signal },
    )
    return () => controller.abort()
  }, [status?.href, status?.stage])
}
