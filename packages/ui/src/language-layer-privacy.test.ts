import { describe, expect, test } from "vitest"

import {
  languageLayerPrivacyCompanion,
  languageLayerPrivacyNotice,
} from "./language-layer-privacy"

const forbidden = [
  /zero data retention/i,
  /zero-retention/i,
  /never stored/i,
  /openrouter/i,
  /vertex/i,
  /gemini/i,
  /google cloud/i,
  /amazon/i,
  /bedrock/i,
  /azure/i,
  /90 days/i,
]

describe("hosted Language Layer privacy copy", () => {
  test("states the permitted training and retention claim without naming a counterparty", () => {
    for (const copy of [
      languageLayerPrivacyNotice,
      languageLayerPrivacyCompanion,
    ]) {
      expect(copy).toMatch(/not used (?:to train|for training)/i)
      // The retention bound is stated in plain words rather than the internal
      // "bounded abuse monitoring" phrasing: kept only on an automated flag,
      // and only for as long as that check runs. The claim is unchanged; the
      // vocabulary is not.
      expect(copy).toMatch(/automated (?:safety )?check/i)
      expect(copy).toMatch(
        /only (?:for a limited time|briefly|for as long as that review takes)/i,
      )
      expect(copy).toMatch(/Help improve coaching/)
      for (const pattern of forbidden) {
        expect(copy).not.toMatch(pattern)
      }
    }
    // The coaching-profile bundle is still disclosed, now in the words a
    // Player reads rather than the internal name for it.
    const coachingContext =
      /short note ChenChess writes about what you are working on/
    expect(languageLayerPrivacyNotice).toMatch(coachingContext)
    expect(languageLayerPrivacyCompanion).toMatch(coachingContext)
    // Still refuses the stronger claim, and still promises the refusal —
    // both in Player-facing words rather than pin vocabulary.
    expect(languageLayerPrivacyNotice).toMatch(
      /not telling you nothing is stored/,
    )
    expect(languageLayerPrivacyNotice).toMatch(
      /stops sending anything and writes your notes itself/,
    )
  })
})
