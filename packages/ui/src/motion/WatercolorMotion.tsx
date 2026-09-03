import { AnimatePresence, motion, useReducedMotion } from "motion/react"

import { brandAssets } from "../assets"

import type { CSSProperties, PropsWithChildren } from "react"

function classNames(...values: Array<string | undefined>): string {
  return values.filter((value) => value != null && value.length > 0).join(" ")
}
export function WatercolorWashPanel({
  children,
  className,
  motionKey,
}: PropsWithChildren<{ className?: string; motionKey: string }>) {
  const reduceMotion = useReducedMotion()

  return (
    <AnimatePresence initial={false}>
      <motion.div
        key={motionKey}
        animate={
          reduceMotion
            ? { opacity: 1 }
            : { opacity: 1, filter: "blur(0px)", scale: 1 }
        }
        className={classNames("chen-wash-panel", className)}
        data-reduced-motion={reduceMotion ? "true" : "false"}
        exit={
          reduceMotion ? { opacity: 0 } : { opacity: 0, filter: "blur(3px)" }
        }
        initial={
          reduceMotion
            ? { opacity: 0 }
            : { opacity: 0, filter: "blur(2px)", scale: 0.992 }
        }
        style={
          reduceMotion
            ? undefined
            : {
                maskImage: `url("${brandAssets.motionMasks.washReveal}")`,
                WebkitMaskImage: `url("${brandAssets.motionMasks.washReveal}")`,
              }
        }
        transition={{ duration: reduceMotion ? 0 : 0.32 }}
      >
        {children}
      </motion.div>
    </AnimatePresence>
  )
}

export function PigmentBloom({
  active,
  position,
}: {
  active: boolean
  position?: Pick<CSSProperties, "left" | "top">
}) {
  const reduceMotion = useReducedMotion()
  const style: CSSProperties = { ...position }
  if (!reduceMotion) {
    style.maskImage = `url("${brandAssets.motionMasks.pigmentBloom}")`
    style.WebkitMaskImage = `url("${brandAssets.motionMasks.pigmentBloom}")`
  }

  return (
    <motion.span
      aria-hidden="true"
      animate={{
        opacity: active ? 0.34 : 0,
        scale: active && !reduceMotion ? 1 : 0.82,
      }}
      className="chen-pigment-bloom"
      data-reduced-motion={reduceMotion ? "true" : "false"}
      initial={false}
      style={style}
      transition={{ duration: reduceMotion ? 0 : 0.26 }}
    />
  )
}

export function DryBrushCircle() {
  const reduceMotion = useReducedMotion()

  if (reduceMotion) return null

  return (
    <motion.span
      aria-hidden="true"
      animate={{ opacity: 0.18, scale: 1 }}
      className="chen-dry-brush-circle"
      data-reduced-motion={reduceMotion ? "true" : "false"}
      initial={{ opacity: 0, scale: 0.94 }}
      style={{
        maskImage: `url("${brandAssets.motionMasks.brushCircle}")`,
        WebkitMaskImage: `url("${brandAssets.motionMasks.brushCircle}")`,
      }}
      transition={{ duration: 0.56 }}
    />
  )
}

export function DiffusionExit({
  children,
  visible,
}: PropsWithChildren<{ visible: boolean }>) {
  const reduceMotion = useReducedMotion()

  return (
    <AnimatePresence initial={false}>
      {visible ? (
        <motion.div
          animate={
            reduceMotion ? { opacity: 1 } : { opacity: 1, filter: "blur(0px)" }
          }
          data-reduced-motion={reduceMotion ? "true" : "false"}
          exit={
            reduceMotion ? { opacity: 0 } : { opacity: 0, filter: "blur(4px)" }
          }
          initial={{ opacity: 0 }}
          style={
            reduceMotion
              ? undefined
              : {
                  maskImage: `url("${brandAssets.motionMasks.diffusionExit}")`,
                  WebkitMaskImage: `url("${brandAssets.motionMasks.diffusionExit}")`,
                }
          }
          transition={{ duration: reduceMotion ? 0 : 0.28 }}
        >
          {children}
        </motion.div>
      ) : null}
    </AnimatePresence>
  )
}
