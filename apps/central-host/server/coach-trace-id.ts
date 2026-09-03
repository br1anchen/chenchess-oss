import { z } from "zod"

const coachTraceIdPattern =
  /^trace:review-session:[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i

export function isCoachTraceId(value: string) {
  return coachTraceIdPattern.test(value)
}

export const coachTraceIdSchema = z.string().refine(isCoachTraceId)
