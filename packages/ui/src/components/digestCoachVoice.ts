import type { DigestCardIdea } from "./DigestCard"

/**
 * The digest's coach voice: what yesterday looked like, and the homework that
 * closes it. Composed from the typed facts the digest already carries — how
 * many Games it covers, and whether each priority is something the Player
 * already does well or something that cost them Games.
 *
 * Deliberately a template, not a model. Which priorities a digest carries,
 * which lessons and drills teach them, and which Games support them are all
 * selected in the Game Review Engine; ADR 0009 (extended by ADR 0035) lets a
 * language layer explain those typed facts but never select, rank, replace or
 * author them. That leaves phrasing as the only job, and phrasing is the one
 * thing a template does with no cost per Player per day, no failure mode on
 * the delivery path, and no output to freeze for the drift gate.
 *
 * A digest with no priorities gets no voice: the homework *is* the priority,
 * so there is nothing to set. `DailyCoachingDigest` renders exactly that card
 * for a day with no eligible Games, and a published digest can carry Games
 * without priorities.
 */

export type DigestCoachVoice = {
  summary: string
  homework: string
}

export function digestCoachVoice(
  gameCount: number | undefined,
  ideas: readonly DigestCardIdea[],
): DigestCoachVoice | null {
  const improvement = ideas.find(({ purpose }) => purpose === "improvement")
  const reinforcement = ideas.find(({ purpose }) => purpose === "reinforcement")
  const leading = reinforcement ?? improvement
  if (gameCount === undefined || !leading) return null

  const played = gameCount === 1 ? "one game" : `${gameCount} games`
  return {
    homework: improvement
      ? `Today's homework: ${improvement.title.toLowerCase()}. Take the lesson, then the drill, and look for it in your next game.`
      : `Today's homework: keep ${leading.title.toLowerCase()} sharp — one drill before you play.`,
    summary: reinforcement
      ? `Nice work in ${played} yesterday — your ${reinforcement.title.toLowerCase()} is landing. Here is what to keep and what to sharpen.`
      : `${played === "one game" ? "One game" : played} yesterday, and one idea is worth your attention before the next.`,
  }
}
