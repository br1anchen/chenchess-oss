import {
  fromExplanationPathRef,
  fromLearningPathRef,
  fromLearningResourceId,
  type CriticalMomentId,
  type ReviewMomentLearningMaterial,
} from "@chenchess/coach-engine-sdk"

export function forkLearningMaterial(
  criticalMomentId: CriticalMomentId,
  ply: number,
): ReviewMomentLearningMaterial {
  return {
    selectionPolicyVersion: "learning-plan-selection/v1",
    resourceCatalogVersion: "learning-resources/2026-08-03",
    tracks: [
      {
        key: { kind: "curriculum", concept: "fork" },
        support: [
          {
            purpose: "reinforcement",
            learningPathRef: fromLearningPathRef("learning-path:fixture-fork"),
            criticalMomentId,
            ply,
            basis: {
              kind: "decisionExplanation",
              explanationPathRef: fromExplanationPathRef(
                "sha256:0000000000000000000000000000000000000000000000000000000000000000",
              ),
            },
          },
        ],
        resources: [
          {
            resourceId: fromLearningResourceId("lichess:practice:Qj281y1p"),
            role: "learn",
            kind: "practiceModule",
            title: "The Fork",
            canonicalUrl:
              "https://lichess.org/practice/fundamental-tactics/the-fork/Qj281y1p",
          },
          {
            resourceId: fromLearningResourceId("lichess:puzzles:fork"),
            role: "drill",
            kind: "puzzleStream",
            title: "Fork",
            canonicalUrl: "https://lichess.org/training/fork",
          },
        ],
      },
    ],
  }
}
