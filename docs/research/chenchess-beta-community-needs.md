# ChenChess beta community needs

Research date: 2026-08-02

This note synthesizes the
[shared ChenChess research chat](https://chatgpt.com/s/t_6a6f52a7313081919ddbfe27e7afab88)
and primary or first-party material from chess federations, chess products, and
practitioners. It does not assess the repository implementation or issue
pipeline.

## Recommendation

Release the first community beta as an **adult-first, completed-game,
single-player learning product**:

> Import one finished game, discuss at most a few critical decisions, leave
> with one defensible lesson and one way to practise it.

The approximately 1000-Elo casual self-learner is the primary user. Advanced
players and adult/junior coaches should be design partners who test chess
correctness, explanatory depth, and coach control. Commentators should test
whether the board and explanations communicate clearly, but a professional
live-broadcast workflow should not be promised by this beta.

This is deliberately narrower than "AI coach for the whole chess community."
The evidence shows that the surrounding capabilities are already mature
products in their own right:

- Chess.com Game Review supplies a graph, move classifications, key moves,
  coach explanations, retry, and self-analysis
  ([official help](https://support.chess.com/en/articles/8584089-how-does-game-review-work)).
- Lichess offers unlimited analysis-board use, cloud analysis, "learn from your
  mistakes," persistent Studies, Chess Insights, and puzzles sourced from user
  games
  ([official feature matrix](https://lichess.org/features)).
- Aimchess already aggregates games into weakness reports, personalized study
  plans, and drills made from the player's own mistakes
  ([official product page](https://aimchess.com/)).
- DecodeChess already markets natural-language explanations of engine moves,
  threats, plans, and position concepts
  ([official feature page](https://decodechess.com/features/)).

ChenChess therefore cannot differentiate merely by adding explanations,
critical moments, longitudinal statistics, or personalized puzzles. Its
credible differentiation is the combination of:

1. inspectable, validated chess facts;
2. a conversation that elicits the player's own goal and thought process;
3. a small learning episode produced from that evidence; and
4. memory that stores only reviewable, user-confirmed learning claims and
   improves later coaching.

That differentiation is still a hypothesis until beta users retain the lesson
and find later reviews more relevant.

## Synthesis of the shared research chat

The chat's substantive proposal is:

- **Promise:** understand how the player thinks, rather than merely score how
  the player moved.
- **Principles:** coaching over analysis; one lesson per game; conversation
  before explanation; durable player memory.
- **Layers:** Game Import -> Chess Intelligence -> Coach Intelligence ->
  Learning Intelligence -> Player Memory. Chess Intelligence produces facts;
  the coaching layer interprets those facts and must not invent chess.
- **Core object:** a `LearningEpisode` containing the position, player goal,
  candidate moves, player thought, misconception, corrected concept,
  discussion, alternative line, practice position, review date, and mastery
  state.
- **First release:** PGN import, critical-moment detection, interactive review,
  alternative exploration, engine-backed explanations, and a summary.
- **Later sequence:** multi-game trends; player memory; spaced practice;
  reasoning graphs; sparring; a human-coach workspace; and community learning
  episodes.
- **Success measures:** time to first insight, review completion, delayed
  recall, non-recurrence of a weakness, and useful follow-up questions—not
  engine depth or annotation count.

Three parts need qualification before they become product requirements.

First, cross-game weakness analysis and game-derived practice are already
competitive baselines. Chess.com's current Insights surfaces results,
accuracy, phases, openings, tactics, and changes over time
([official help](https://support.chess.com/en/articles/8708925-what-is-insights-on-chess-com));
Aimchess analyzes recent games in aggregate and turns weaknesses into targeted
training ([official product page](https://aimchess.com/)).

Second, a "thinking graph" cannot be inferred reliably from engine moves alone.
The product must distinguish:

- what the player explicitly said;
- what deterministic chess evidence establishes;
- what the coach tentatively hypothesizes; and
- what the player or human coach later confirms.

Third, durable labels such as "panics," "is overconfident," or "is passive" are
profiling, not chess facts. For children in particular, the UK Information
Commissioner's Office says profiling should be off by default unless there is a
compelling child-interest reason, with transparent controls and safeguards
([official Children's Code guidance](https://ico.org.uk/for-organisations/uk-gdpr-guidance-and-resources/childrens-information/childrens-code-guidance-and-resources/how-to-use-our-guidance-for-standard-one-best-interests-of-the-child/best-interests-framework/profiling-for-content-delivery/)).
The beta should retain a confirmed chess-learning claim ("I stopped calculating
after I saw the fork") rather than assign a psychological trait ("panics under
pressure").

## Persona jobs and pain points

| Persona                       | Jobs to be done                                                                                                                                                                                        | Pain points the beta must address                                                                                                                                                                              | Appropriate beta role                                               |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| ~1000 Elo casual self-learner | Find the turning point; understand why a natural move failed; learn one reusable idea; try the decision again; know what to practise next                                                              | Engine numbers and long variations are not lessons; the player may not know what question to ask; too many labels feel like a report card; unexplained terminology and confident-but-wrong prose destroy trust | Primary user and usability/retention cohort                         |
| Advanced player               | Reconstruct candidate moves; test a concrete hypothesis; inspect exact lines, evaluation assumptions, and alternatives; annotate or export work; receive concise rather than beginner-level prose      | Simplification can erase the tactical reason; a single engine line hides viable choices; unsupported strategic labels are easy to notice; lack of PGN/export makes the work disposable                         | Chess-correctness, depth, and adversarial-review cohort             |
| Adult or junior club coach    | Find instructive moments quickly; adapt to age, strength, goals, and curriculum; ask before revealing; hide or show engine evidence; correct AI commentary; share homework; see whether a lesson stuck | Preparation time, mixed student levels, AI choosing the wrong teaching point, loss of teacher authority, no edit/override path, and safeguarding/privacy obligations                                           | Human-oversight and workflow cohort; not merely another end user    |
| Chess commentator             | Select the relevant game rapidly; keep the real position distinct from an analysis variation; explain threats and alternatives with visual cues; maintain a coherent audience narrative                | Multi-game information overload, stale feeds, accidentally presenting an analysis branch as the game, cluttered layouts, and narration that is technically correct but not audience-appropriate                | Post-game communication evaluator in beta; full live workflow later |

The FIDE Trainers' Commission explicitly distinguishes instruction by learner
level: a Developmental Instructor typically teaches beginners through 1200,
while progressively stronger titles serve 1201–1700, 1701–2200, and 2200+
players. Its title evaluation also considers student results, experience,
professional teaching skills, and an examination—not playing rating alone
([official trainer-seminar guidance](https://trainers.fide.com/trg-online-seminars/)).
One undifferentiated "coach personality" is therefore not enough.

First-party coaching guidance similarly emphasizes age- and level-appropriate
instruction, frequent analysis of the student's own games, and alternative
ideas, while noting that teaching experience can matter more than a title for
beginner and intermediate students
([Chess.com practitioner guidance](https://www.chess.com/article/view/how-to-find-a-chess-coach)).

### Casual self-learner requirements

The default review should:

1. show no more than three critical decisions;
2. ask what the player noticed, feared, expected, or calculated before
   revealing an answer;
3. explain one causal idea in rating-appropriate language;
4. let the player retry the position without the best move already exposed;
5. compare at least one plausible alternative rather than only the engine's
   top move; and
6. finish with one lesson and one short practice action.

This matches the strongest parts of the shared-chat thesis while meeting the
existing Game Review baseline: guided key moves, explanations, move prediction,
and retry are already expected
([Chess.com Game Review](https://support.chess.com/en/articles/8584089-how-does-game-review-work)).
The value must be the quality of the dialogue and retained lesson, not the
presence of those controls.

### Advanced-player requirements

Advanced users need an evidence view beneath the coaching view:

- an optional thoughts-first pass that records the player's candidates and
  evaluation before any engine answer is shown;
- complete move navigation, not only selected moments;
- candidate moves and reproducible principal variations;
- evaluation and analysis provenance, including relevant engine settings;
- explicit separation of fact, model interpretation, and player/coach
  hypothesis;
- alternative-line exploration without losing the real game line;
- comments and annotated PGN export; and
- a concise mode that does not explain basic vocabulary.

Chess.com's self-analysis already exposes evaluation, engine lines, suggestion
arrows, threats, engine choice, depth, and number of lines, and lets users save
comments and alternative lines
([official analysis help](https://support.chess.com/en/articles/8583757-how-do-i-use-game-analysis)).
Lichess Study saves variations, comments, symbols, and arrows; supports
real-time collaboration and multi-chapter PGN/FEN import; permits puzzle-like
hidden moves; and exports PGN
([official Study introduction](https://lichess.org/@/lichess/blog/study-chess-the-lichess-way/V0KrLSkA)).
If ChenChess hides its evidence or traps the result in its own UI, an advanced
player has no reason to trust or adopt it.

The thoughts-first ordering is substantive, not cosmetic. A National Master and
former school chess teacher describes annotating without an engine first
because an engine answer is almost impossible to "unsee"; he then compares his
recorded thoughts and lines with the engine and tracks recurring
decision-making patterns
([first-party practitioner workflow](https://www.chess.com/article/view/how-to-annotate-your-games-for-chess-improvers)).

### Coach requirements

For the first beta, a coach does not need a full school-management system. The
minimum useful coach workflow is:

- import a student's completed game with explicit permission;
- see why each critical moment was selected;
- hide evaluation/lines while asking the student, then reveal them;
- edit, approve, reject, or replace AI-authored teaching text;
- retain the student's stated reasoning separately from AI inference;
- export or share the resulting lesson and practice position; and
- keep each student's data and permissions isolated.

This is a deliberately smaller baseline than Chess.com Classroom, which
supports participants, multiple games, FEN/PGN, coach-only engine visibility,
engine/depth/line controls, participant move permissions, chat, and media
controls
([official Classroom help](https://support.chess.com/en/articles/8708915-how-do-i-use-classroom-on-chess-com)).
Lichess Study likewise supports private or public lessons, contributors,
students, persistent annotations, and hidden-move exercises
([official Study introduction](https://lichess.org/@/lichess/blog/study-chess-the-lichess-way/V0KrLSkA)).

A roster, billing, scheduling, video, group classroom, and curriculum builder
can wait. Human editorial authority, provenance, sharing, and student isolation
cannot wait if coaches are invited to use the beta.

### Commentator requirements and boundary

Professional commentary is not "game review with a larger board." Chess.com's
first-party broadcast guide treats the event game list as the commentator's
main selection tool and includes round/player filters, recent and focus lists,
clocks, engine-line control, a visually distinct analysis branch, rapid return
to the live position, colored arrows/highlights, capture-friendly layouts, and
a separate live-tracking board
([official broadcast guide](https://www.chess.com/article/view/events-page-broadcast-guide)).
Lichess Broadcasts consume live-updating PGNs, support delays, grouping,
embeds, moderation, official/private undelayed feeds, viewer statistics, and
feed-error monitoring
([official Broadcast help](https://lichess.org/broadcast/help)).

The beta should support commentators only as follows:

- import a completed PGN;
- navigate real moves and clearly marked alternative lines;
- use a clean, stable board with arrows/highlights;
- copy or export a critical-position package and factual summary; and
- obtain source/evidence details for any on-air claim.

Live feeds, multi-board navigation, clocks, standings, production overlays,
stream synchronization, and event operations are a separate future surface.
Without them, market the beta as a post-game explanation aid, not a broadcast
desk.

## Competitive baseline and strategic response

| Capability users can already obtain                                                                           | Primary evidence                                                                                                                                                                                      | ChenChess response                                                                                               |
| ------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| Guided post-game graph, classifications, key moves, explanations, retry, and self-analysis                    | [Chess.com Game Review](https://support.chess.com/en/articles/8584089-how-does-game-review-work)                                                                                                      | Match the completed-game journey; compete on causal dialogue and learning outcome                                |
| Deep/free engine analysis, cloud analysis, mistakes, Studies, longitudinal insights, and game-derived puzzles | [Lichess features](https://lichess.org/features)                                                                                                                                                      | Do not compete on engine access or generic tools; keep evidence inspectable and portable                         |
| Multi-game patterns, weaknesses, and targeted drills                                                          | [Aimchess](https://aimchess.com/) and [Chess.com Insights](https://support.chess.com/en/articles/8708925-what-is-insights-on-chess-com)                                                               | Delay dashboards until the single-game learning loop works; make later memory reasoning-aware and user-confirmed |
| Natural-language explanations of moves, threats, plans, and concepts                                          | [DecodeChess](https://decodechess.com/features/)                                                                                                                                                      | Prove grounding, conversational diagnosis, and correction rather than claim explanations are novel               |
| Interactive course content, immediate feedback, and spaced review                                             | [Chess.com Courses/Chessable announcement](https://www.chess.com/news/view/announcing-courses)                                                                                                        | Generate a small practice item from the player's own confirmed lesson and measure delayed recall                 |
| Persistent collaborative teaching boards with coach controls and export                                       | [Chess.com Classroom](https://support.chess.com/en/articles/8708915-how-do-i-use-classroom-on-chess-com) and [Lichess Study](https://lichess.org/@/lichess/blog/study-chess-the-lichess-way/V0KrLSkA) | Integrate with/export to existing coach workflows before building a full classroom                               |
| Professional live-event and commentary tooling                                                                | [Chess.com broadcast guide](https://www.chess.com/article/view/events-page-broadcast-guide) and [Lichess Broadcast help](https://lichess.org/broadcast/help)                                          | Keep live broadcast out of the beta; provide only post-game presentation/export                                  |

The shared chat's "what not to build" list remains sound for beta: no opening
explorer, engine cloud, puzzle rush, live game analysis, repertoire builder, or
tournament management. Each would broaden the product into a mature incumbent's
domain before ChenChess has proved its learning loop.

## Safety, child coaching, privacy, and fair play

### Adult-first release

The safest first community beta is restricted to adults. Coaches who work with
children may evaluate workflows using adult-owned or consented, de-identified
test material, but direct child accounts and unmediated child/AI chat should
remain disabled until the child-safety contract is implemented and reviewed.

If direct use by minors is added, minimum requirements include:

- jurisdiction-aware age handling and verifiable guardian consent;
- guardian access, correction, export, and deletion controls;
- no public profile, direct messaging, open enrollment, or discoverability by
  default;
- minimal data collection, explicit retention, and deletion schedules;
- purpose-specific controls for learning personalization versus other
  profiling;
- profiling off by default for children unless a documented child-interest
  assessment justifies it;
- a visible reporting path and an operational incident-response owner; and
- age-appropriate, non-shaming feedback with no mental-health diagnosis or
  durable personality label.

Lichess provides a concrete chess-product baseline: Kid Mode blocks general
direct messages, forums, blogs, streams, and videos, while class messaging is
limited to the teacher and classmates
([official Kid Mode](https://lichess.org/page/kid-mode)). Its Class product
gives the teacher administrative control, safe generated credentials,
restricted chat by default, homework distribution, and progress monitoring
([official Lichess Class](https://lichess.org/page/class)).

The FTC's COPPA guidance requires covered services to give parents notice and
obtain verifiable consent before collecting children's personal information,
let parents review/delete it and stop further collection, protect it, minimize
collection, and retain it only as long as needed
([official COPPA FAQ](https://www.ftc.gov/business-guidance/resources/complying-coppa-frequently-asked-questions)).
Chess.com currently restricts social features pending parental consent and
directs under-13 players toward ChessKid
([official parental-consent help](https://support.chess.com/en/articles/11070988-why-do-i-need-parental-consent)).
These are useful product baselines, not a substitute for jurisdiction-specific
legal review.

Chess-specific safeguarding is broader than privacy. US Chess Safe Play covers
sexual misconduct, bullying, hazing, harassment, emotional misconduct,
physical misconduct, mandatory training/background screening, and reporting
([official policy](https://new.uschess.org/sites/default/files/media/documents/us-chess-safe-play-guidelines.pdf)).
The English Chess Federation expects safeguarding qualifications when coaches
serve children or vulnerable adults
([registered-coach scheme](https://www.englishchess.org.uk/new-ecf-registered-coaches-scheme/))
and provides explicit reporting channels
([safeguarding page](https://www.englishchess.org.uk/safeguarding-and-the-ecf/)).
FIDE's current Ethics and Disciplinary Code says people in positions of trust
must not abuse coach/official power, must avoid one-to-one or unobserved
situations with a minor, and must communicate with minors openly rather than by
private message
([official code](https://handbook.fide.com/files/handbook/EthicsAndDisciplinaryCode2022.pdf)).
It also requires feedback to be honest, positive, factual, constructive, and
open to the player's response.
A future coach marketplace would therefore require credential/safeguarding
verification and reporting operations; a profile claiming "coach" is
insufficient.

### Fair-play boundary

ChenChess must analyze only completed games in its player-coaching surface.
Chess.com prohibits engines and outside advice in ongoing live games and
prohibits engine/person advice for a specific Daily game in progress
([official Fair Play help](https://support.chess.com/en/articles/8568369-what-do-i-need-to-know-about-fair-play-on-chess-com)).
Lichess likewise prohibits external assistance that improves a player's
knowledge or calculation while the game is ongoing
([official Fair Play rules](https://lichess.org/page/fair-play)).

The beta should:

- require a completed-game result in imported PGN/provider data;
- reject or quarantine unfinished and ambiguous imports;
- state visibly that the tool is for post-game learning;
- avoid live-page overlays, automatic current-position capture, and
  move-recommendation notifications; and
- keep any future broadcaster feed/role technically and operationally separate
  from the player coach.

## Evidence-backed beta acceptance criteria

The thresholds below are proposed product gates, not federation or legal
standards. They turn the cited jobs and competitive baselines into testable
release conditions.

### 1. Scope and fair play

- A supported completed standard-chess PGN and at least one documented provider
  game source can start a review without manual identifier handling.
- Malformed, unsupported, ambiguous-side, and unfinished games fail with a
  specific recovery path; no such input reaches coaching generation.
- Every player-facing entry point says "completed game" or "post-game," and an
  in-progress-game test suite demonstrates fail-closed behavior.

### 2. Chess truth and trust

- Every canonical coaching claim links to the position, candidate move,
  variation, evaluation, motif/theme fact, or player statement that supports
  it.
- Accuracy and performance metrics are labeled as engine/formula-derived
  indicators, not school grades, player Elo estimates, or evidence of
  cheating. Lichess explains that position complexity distorts grade-like
  readings, high accuracy is not proof of cheating or GM-level play, and sites
  legitimately differ because formulas, engines, and depths differ
  ([official accuracy documentation](https://lichess.org/page/accuracy)).
- The release corpus contains zero illegal moves, impossible board states,
  wrong results, or prose that contradicts the admitted chess facts.
- Facts, AI interpretations, player statements, and coach hypotheses are
  visibly distinct and independently removable.
- Users can inspect at least one reproducible alternative line for every
  criticized critical decision.
- Timeouts, cancellation, and partial failures never publish a half-grounded
  lesson as canonical.

### 3. Casual-player learning loop

- The default journey presents at most three critical decisions and one key
  lesson, with a clear path to the full game.
- At least one moment asks for the player's thought before revealing the
  engine-backed answer.
- The player can retry a move without a pre-revealed arrow or best move, receive
  bounded feedback, and then inspect why a plausible alternative works or
  fails.
- The summary contains one reusable concept and one practice action; it does
  not merely repeat move classifications or centipawn changes.
- In moderated usability tests, at least 80% of target casual players complete
  the review unaided and can restate the intended lesson immediately. Delayed
  recall is measured again after 7 and 30 days.

### 4. Advanced-player inspection

- A user can traverse the full game, inspect candidate lines/evaluations and
  provenance, branch without losing the game line, and choose concise
  terminology.
- The review and human annotations export as a standards-compatible PGN, or a
  lossless documented equivalent plus raw PGN.
- Advanced reviewers can flag an incorrect fact, weak critical-moment choice,
  or misleading explanation from the exact position, and the report retains
  enough evidence to reproduce it.

### 5. Human-coach authority

- A coach can see the selection rationale, hide/reveal engine evidence, and
  approve, edit, reject, or replace generated coaching before sharing it as
  coach-authored material.
- Player-stated thinking is never silently rewritten into an inferred trait.
  Proposed memory items require explicit player or coach confirmation and can
  be corrected or deleted.
- A lesson/practice package can be shared or exported without exposing another
  student's data.
- At least one adult coach and one experienced junior coach complete a real
  review-preparation exercise and confirm that the artifact saves preparation
  time without surrendering editorial control.

### 6. Commentator boundary

- If marketed to commentators at all, the beta is explicitly described as a
  **completed-game/post-game aid**.
- A clean board, real-game line, visibly distinct analysis branch, arrows or
  highlights, and factual summary remain stable under screen capture.
- Live-feed, multi-board, clock, standings, and production claims are absent
  until a separate broadcaster acceptance plan meets the established
  Chess.com/Lichess workflow baseline.

### 7. Child-use gate

- Direct minor accounts and unmediated minor/AI conversations remain disabled
  until guardian consent, privacy/data controls, profiling controls,
  safeguarding reporting, and operational response have all passed specialist
  review.
- No release metric or growth experiment is allowed to override this gate.

### 8. Community beta evidence

Before calling the product community-ready, run task-based sessions with at
least:

- five approximately 800–1200 Elo casual players;
- three advanced/titled or strong club players;
- three coaches, including adult and junior-coaching experience; and
- two commentators, streamers, or club-game annotators.

Add an independent safeguarding lead or experienced club safeguarding officer;
chess strength does not substitute for this review.

For each persona, at least 80% should complete its assigned beta job without
operator rescue. Any severe chess-fact error, safeguarding failure, cross-user
data exposure, or live-game assistance path blocks release regardless of the
aggregate score. Rating gain is too noisy and slow to be the initial gate;
review completion, lesson recall, explanation corrections, follow-up quality,
and later recurrence of the confirmed weakness are the first beta measures.

### 9. Accessibility

- The review is keyboard-operable, move quality is not conveyed by color alone,
  notation and coach text work with screen readers, and the critical-moment
  journey does not require drag-and-drop.
- Test the complete import -> moment -> retry -> summary path with at least one
  blind or visually impaired chess player. Lichess's current Blind Mode shows
  that keyboard and screen-reader access can include engine analysis, puzzles,
  study positions, and broadcasts
  ([official tutorial](https://lichess.org/page/blind-mode-tutorial)).

## Service-boundary implications to compare with the implementation

Without prescribing repository structure, the research implies five separable
responsibilities:

1. **Game evidence:** completed-game validation, board state, engine/candidate
   lines, critical-moment facts, and provenance.
2. **Coaching interaction:** questions and explanations that can read evidence
   but cannot silently promote prose to chess fact.
3. **Learning episode:** one confirmed lesson, practice item, follow-up state,
   and mastery evidence.
4. **Player/coach policy and memory:** role/guardian authorization, per-student
   isolation, consent, and explicit confirm/edit/delete rules for remembered
   claims.
5. **Presentation and portability:** player review, coach-editable artifact,
   evidence view, PGN/export, and a post-game presentation projection.

The highest-risk coupling would be allowing generated coaching prose to write
directly into durable player memory. The strongest refactoring test is whether
a factual review can exist without generated prose, a coaching conversation can
fail without corrupting the review, and a memory claim can be rejected without
altering the underlying game evidence.

## Explicitly later

Do not make these beta release blockers:

- a full longitudinal dashboard;
- automated psychological or emotional trait inference;
- spaced-repetition scheduling beyond one exportable practice item;
- coach discovery, billing, rosters, curriculum, group classroom, or video;
- community/shared learning episodes;
- opening explorer or repertoire construction;
- AI sparring;
- live-game assistance; or
- professional tournament broadcast production.

The first beta earns the right to build those only by showing that one grounded
conversation produces a lesson the player understands, remembers, and finds
useful in a later game.
