# Issue #323 Daily Coaching staging and soak

Issue #323 is a human-in-the-loop release boundary. The repository prepares
the fail-closed gates and privacy-safe artifact shape; it cannot manufacture
provider authorization, deployed journeys, operator email receipt, or seven
days of elapsed beta operation.

## Automated conformance

`tooling/scripts/daily-coaching-conformance.ts` is the release manifest for
the 18 named journeys and five correctness invariants. Each entry points to
one or more exact compiled Coach Engine test names. The scoped Coach Engine
gate runs the real Rust suite first and then enumerates the compiled inventory;
a deleted or renamed assertion therefore fails the release even if every
remaining test is green.

The artifact records these journey names:

1. `initialBackfillOwedRemainder`
2. `zeroEligibleGameDay`
3. `oneThroughTenEligibleGames`
4. `deterministicTenGameCap`
5. `mergedTwoProviderCap`
6. `partialReviewPublishesSuccesses`
7. `tickAndArrivalIdempotency`
8. `connectTimezoneCaptureAndFallback`
9. `archivePublicationOrdering`
10. `emailDeliveryLifecycle`
11. `accountDeletionErasesDailyCoaching`
12. `authenticatedFrozenReviewIsolation`
13. `profileUnavailableLifecycle`
14. `healthyProviderPublishesDuringPeerFailure`
15. `chessComDailyCorrespondenceEligibility`
16. `chessComKindNamespacedIdentity`
17. `chessComArchiveMonthBoundary`
18. `reviewedGameSearchContract`

The invariant names are `exactFrozenLearningPlans`, `maximumTwoPriorities`,
`gameAppearsInOnlyOneDigest`, `zeroEligibleDayPublishesNoDigest`, and
`oneEmailPerDigest`.

## Chess.com connections

Chess.com Daily Coaching is unconditional. Staging and production accept
Chess.com connections the same way they accept Lichess. There is no
availability environment variable and no environment-specific reject path.

Written Chess.com authorization is a human decision for the soak. The request
must state that a closed invite-only beta with third-party testers is running.
Authorization does not change application configuration.

## Deployed checks

Against the exact staging revision, a human records successful checks named
`mailProviderSend`, `mcpClientRoundTrip`, and `authenticatedDeepLink` in the
certification artifact. Use a valid verified test address for the real mail
send, a supported MCP client with real authentication, and a fresh browser
session for the digest deep link. The artifact retains only the check names,
not addresses, tokens, provider responses, Player identifiers, Game content,
or screenshots containing private conversation text.

## Seven-day beta soak

The soak is seven consecutive completed beta days with at least two Players
holding live Lichess and Chess.com connections concurrently. For each day:

- confirm the Daily Operator Digest arrived;
- reconcile every terminal Run and require zero `Abandoned` Runs;
- explain every `Skipped` Run, with zero unexplained skips;
- check the mail provider for zero spam complaints and zero hard bounces to
  valid verified addresses.

Across the seven days, record at least one `Published` day attributable to
each provider, one `NoDigest` day, one deliberate `profileUnavailable` set and
silent-clear drill, and one deliberate mid-Run restart that produces takeover
and exactly one published digest. Do not use a load test or a product
engagement threshold as a gate.

The artifact contains counts and pass names only. Keep the daily reconciliation
worksheet and sensitive provider details in the operator's approved evidence
store, not in Git, the certification JSON, or a GitHub comment.

## Certification and promotion

The schema-version-2 staging artifact adds `dailyCoaching` to the existing
staging certification. It must enumerate the exact conformance sets, deployed
checks, soak counts and drills, and a distinct immutable prior
production revision as `rollbackRevision`.

Validate it against the exact candidate:

```sh
bun run review-session:certify -- \
  --input <staging-certification.json> \
  --revision <exact-40-character-commit>
```

Only after the validator passes may the operator follow ADR 0052: fast-forward
`prod` to that immutable commit and retain the recorded rollback revision.
The production version is that SHA. Production autodeploy is enabled from
`prod`.
