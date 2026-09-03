import { useEffect, useState, type ReactNode } from "react"

import {
  DialogHeader,
  VStack,
  WatercolorCard,
  WatercolorDialog,
  type WatercolorDialogBackdrop,
} from "@chenchess/ui"

import { watercolorOverlayStyles } from "./watercolorOverlay.styles"

const compactViewport = "(max-width: 860px)"

export function WatercolorOverlay({
  backdrop = "paper",
  children,
  onOpenChange,
  open,
  title,
}: {
  backdrop?: WatercolorDialogBackdrop
  children: ReactNode
  onOpenChange: (open: boolean) => void
  open: boolean
  title: string
}) {
  const compact = useCompactViewport()
  return (
    <WatercolorDialog
      backdrop={backdrop}
      data-overlay-placement="dialog"
      isOpen={open}
      maxHeight={compact ? "85dvh" : "80vh"}
      onOpenChange={onOpenChange}
      padding={compact ? 1 : undefined}
      purpose="info"
      width={compact ? "min(36rem, calc(100vw - 0.5rem))" : "36rem"}
      xstyle={watercolorOverlayStyles.dialog}
    >
      <DialogHeader onOpenChange={onOpenChange} title={title} />
      <WatercolorCard
        frame={false}
        padding="comfortable"
        xstyle={watercolorOverlayStyles.card}
      >
        <VStack
          data-watercolor-overlay-body=""
          gap={4}
          hAlign="stretch"
          xstyle={watercolorOverlayStyles.body}
        >
          {children}
        </VStack>
      </WatercolorCard>
    </WatercolorDialog>
  )
}

function useCompactViewport(): boolean {
  const [compact, setCompact] = useState(readCompactViewport)
  useEffect(() => {
    const media = window.matchMedia(compactViewport)
    const sync = () => setCompact(media.matches)
    sync()
    media.addEventListener("change", sync)
    return () => media.removeEventListener("change", sync)
  }, [])
  return compact
}

function readCompactViewport(): boolean {
  return window.matchMedia(compactViewport).matches
}
