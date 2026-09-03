import { Theme } from "@astryxdesign/core/theme"
import type { PropsWithChildren } from "react"

// The built theme, not `inkWash.ts`. A built theme carries `__built`, which
// tells `Theme` the token CSS already ships in the stylesheet and stops it
// injecting a duplicate `<style>` at runtime.
import { inkWashTheme } from "./generated/ink-wash"

export function ChenTheme({ children }: PropsWithChildren) {
  return (
    <Theme mode="light" theme={inkWashTheme}>
      {children}
    </Theme>
  )
}
