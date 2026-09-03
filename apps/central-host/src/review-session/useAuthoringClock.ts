import { useEffect, useState } from "react"

/**
 * A bounded wait, never a typewriter: the note appears whole or not at all.
 * Counts down to zero and stays there so the caller can settle unpublished.
 */
export function useAuthoringClock(seconds: number, running: boolean) {
  const [remaining, setRemaining] = useState(seconds)
  useEffect(() => {
    if (!running) return
    setRemaining(seconds)
    const timer = window.setInterval(() => {
      setRemaining((value) => (value <= 1 ? 0 : value - 1))
    }, 1000)
    return () => window.clearInterval(timer)
  }, [running, seconds])
  return remaining
}
