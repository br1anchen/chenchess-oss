import { WatercolorNotice } from "@chenchess/ui"
import * as stylex from "@stylexjs/stylex"

export type AuthNoticeStatus = "info" | "warning" | "error" | "success"

const wash = stylex.create({
  success: {
    "--watercolor-card-accent": "var(--color-success)",
    "--watercolor-card-paper":
      "color-mix(in srgb, var(--color-success) 18%, var(--color-paper-raised))",
  },
  error: {
    "--watercolor-card-accent": "var(--color-error)",
    "--watercolor-card-paper":
      "color-mix(in srgb, var(--color-error) 18%, var(--color-paper-raised))",
  },
  warning: {
    "--watercolor-card-accent": "var(--color-text-secondary)",
    "--watercolor-card-paper":
      "color-mix(in srgb, var(--color-mist) 32%, var(--color-paper-raised))",
  },
  info: {
    "--watercolor-card-paper": "var(--color-paper-raised)",
  },
})

/** Form and interstitial success / failure on auth surfaces. */
export function AuthNotice({
  detail,
  message,
  status,
}: {
  detail?: string
  message: string
  status: AuthNoticeStatus
}) {
  return (
    <WatercolorNotice
      detail={detail}
      glyph={noticeGlyph(status)}
      heading={message}
      padding="compact"
      role={noticeRole(status)}
      tone={noticeTone(status)}
      xstyle={wash[status]}
    />
  )
}

function noticeGlyph(status: AuthNoticeStatus) {
  switch (status) {
    case "error":
    case "warning":
      return "!"
    case "success":
      return "✓"
    case "info":
      return "i"
    default: {
      const _exhaustive: never = status
      return _exhaustive
    }
  }
}

function noticeRole(status: AuthNoticeStatus) {
  switch (status) {
    case "error":
    case "warning":
      return "alert"
    case "success":
    case "info":
      return "status"
    default: {
      const _exhaustive: never = status
      return _exhaustive
    }
  }
}

function noticeTone(status: AuthNoticeStatus) {
  switch (status) {
    case "success":
      return "bamboo"
    case "error":
      return "vermilion"
    case "warning":
      return "mist"
    case "info":
      return "paper"
    default: {
      const _exhaustive: never = status
      return _exhaustive
    }
  }
}
