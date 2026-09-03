export const retentionPreferenceDescription =
  "Keeps a pseudonymized copy of your reviews for up to 12 months so we can improve coaching. Turning it off stops new copies immediately, and the copies we already hold are deleted or withdrawn before the change saves. Turning it back on covers only later reviews — nothing deleted comes back."

export const retentionDisclosureDescription =
  "When this is on, ChenChess keeps one pseudonymized copy of each review for up to 12 months to improve coaching. It can hold normalized moves, the coaching we generated, and the details needed to reproduce it — never raw PGN, account identifiers, your name, your usernames, or game links. Turning it off stops new copies immediately, and the copies we already hold are deleted or withdrawn before the change saves. Turning it back on covers only later reviews — nothing deleted comes back."

/**
 * Submit-time disclosure for Review Feedback (ADR 0049). Submitting feedback
 * with the Quality Capture Preference off induces a `feedback-induced` capture
 * of that generation, so the Player is told before the button, not after it.
 */
export const reviewFeedbackDisclosureDescription =
  "Sending feedback keeps one pseudonymized copy of this coaching comment so we can check its quality — even when keeping copies is turned off."
