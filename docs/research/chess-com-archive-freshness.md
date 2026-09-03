# Chess.com monthly archive freshness against the daily run window

Research date: 2026-08-09

This note measures how long a finished Chess.com Game takes to become readable
in its documented monthly PubAPI archive, and checks that latency against the
Daily Coaching run window fixed by
[Define the durable Daily Coaching workflow](#222).
It reports evidence only; the eligibility and window policy it informs is
decided elsewhere.

## Finding

**Publication lag is seconds, not hours, and the existing window already
absorbs it by two orders of magnitude.** Across 44 games observed appearing in
live archives, lag from `end_time` to first readable archive entry ran
3.4 s – 92.6 s (median 16.5 s, p90 32.7 s); nothing exceeded 93 s. The Daily
Window closes at local midnight and becomes due at 02:00 local plus a
0–60 minute spread, so the earliest read of the last Game of the previous local
day happens **≥ 2 hours** after it finished.

No lag allowance is needed, and no re-check pass is justified by ordinary
publication lag.

The residual risk is not latency but **archive outage**: Chess.com has
previously served empty or incomplete archives for days (see below). Because a
Daily Window is never carried forward, such an outage silently costs those
days. That is an operational-alerting question, not a window-sizing one.

Two mechanical corrections for the daily run fall out of the measurements and
are stated under [Consequences](#consequences): the run must address archive
months **directly** rather than through the archives list, and on a boundary
day it must read **both** months that the local previous day spans.

## Method

All requests were anonymous `GET`s to documented endpoints, serial, with a
contact-bearing User-Agent, on 2026-08-09 between 07:56 and 08:52 UTC.

1. Build a pool of currently-active accounts from `/pub/leaderboards`
   (`live_bullet`, `live_blitz`, `live_rapid`, `daily`, `daily960`) plus live
   `/pub/streamers` — 222 accounts.
2. Sweep the pool, recording every game already present in the current-month
   archive as baseline, and mark an account **hot** when its newest game
   finished under 10 minutes ago.
3. Tight-poll hot accounts' current-month archives continuously, and for each
   game seen for the first time record
   `lag = observed_at - end_time`, its `time_class`, and whether the entry
   carried a `pgn`.

Lag is therefore an **upper bound** with poll-cycle granularity of a few
seconds, not an exact publication instant. A first sweep of 60 random GMs was
discarded: a random titled sample is almost entirely idle and produced no
appearances in 4 minutes.

Raw observations: 44 records, `time_class` distribution — bullet 26, rapid 9,
blitz 8, daily 1.

## Evidence

### Measured publication lag

| Statistic  | Lag (s) |
| ---------- | ------- |
| minimum    | 3.4     |
| median     | 16.5    |
| p90        | 32.7    |
| maximum    | 92.6    |
| over 60 s  | 4 of 44 |
| over 120 s | 0 of 44 |

The single **Daily** (correspondence) game observed appeared at 20.1 s, in the
same range as Live games. One data point is not a distribution, but nothing
suggests Daily games take a separate slower path — consistent with the
documentation describing one archive of "Live and Daily Chess games that a
player has finished".

No game was observed appearing late — that is, with a lag on the order of
hours, which the sweep would have recorded as a large `lag_s` for an account
polled repeatedly across the 50-minute window.

### The archive is live, despite the documentation

The published documentation's general caching note ("The endpoints refresh at
most once every 12 hours", elsewhere 24) does **not** describe the monthly
archives. Observed response headers on
`/pub/player/{u}/games/2026/08` (`hikaru`, 07:56:46 UTC):

```
cache-control: public, max-age=5
last-modified: Sunday, 09-Aug-2026 07:54:32 GMT+0000
etag: W/"9e143f3e8785e3239b8584e91be647ac"
cf-cache-status: REVALIDATED
```

A 5-second edge TTL and a `Last-Modified` two minutes old match the measured
seconds-scale lag. `ETag`/`Last-Modified` are present and usable for
conditional requests, but at a 5 s TTL they buy the daily run nothing — it
reads each archive once per window.

Chess.com staff describe the write path as deliberately asynchronous: "The
archive is updated asynchronously, so you can see some delay between a game
finishing and showing on your archive … It's a load mitigation strategy;
there's almost 11 million games a day". Users on that thread report
"30 seconds to 1 minute", occasionally "a couple minutes" — consistent with the
measurements here.

### Games are bucketed by end time, in UTC

`end_time` equals the PGN's `EndDate`/`EndTime` UTC headers, and a game that
crosses a UTC day boundary belongs to the day it **ended**. Observed in
`hikaru`'s August archive:

```
UTCDate 2026.08.04 23:57:28  ->  EndDate 2026.08.05 00:01:38
end_time = 2026-08-05T00:01:38Z
```

The July archive for the same account contained games ending between
2026-07-01T12:05:23Z and 2026-07-31T20:48:51Z, and the August archive none
earlier than 2026-08-01T15:32:51Z — no entry sat outside its bucket's UTC
month. The month bucket therefore follows `end_time` in **UTC**, not the
Player's timezone and not the start time.

### A month with no games is absent from the archives list but still readable

`/pub/player/{u}/games/archives` lists only months that contain games — a
January with no play is simply missing. Addressing such a month directly still
answers `200` with `{"games": []}`.

Separately, the archive URL is **case-sensitive on the username**: a mixed-case
username answers `301` with an error body naming the lowercase URL. This
matches the lowercased provider identity fixed by
[Extend the Playing Profile Connection lifecycle to Chess.com](#298).

### Missing PGN is a non-chess variant, not a publication race

One of the 44 appearances carried no `pgn`. Re-read 1 h 45 m later, it still
carried none — it is `rules: bughouse`, which the feed already excludes by
`rules != "chess"`. No standard-chess game was observed appearing without its
PGN. The documented "if PGNs are not properly calculated the games archives
cache will expire in 5 minutes in order to retry sooner" implies a
PGN-less standard game is possible, but it self-heals within minutes — far
inside the 2-hour grace — so the discovery-time filter fixed by
[Define the Chess.com Game Import source and provenance](#297)
never has to serve as a race guard.

### Archives have failed for days before

In November 2021 whole monthly archives returned empty for many accounts across
several days; staff confirmed a live-games regression requiring an engineering
fix plus cache refreshes, not a caching delay. This is the failure mode worth
designing for, and it is invisible to a lag measurement.

## Consequences

Stated as facts for the tickets that own the decisions, not as decisions.

1. **No lag allowance.** A 2-hour minimum grace against a ≤93 s observed
   worst case leaves ~77× headroom. Shrinking the grace toward the Player's
   local midnight is what would need re-justification, not keeping it.
2. **No re-check pass for lag.** Re-reading the archive later would find the
   same set. It would only pay off during an archive outage, which needs
   detection and operator response rather than a second read.
3. **Read months by construction, not from the archives list.** Resolve the
   local previous day to a UTC interval and read exactly the archive months
   that interval intersects. The archives list omits empty months, so a run
   that walks the list can miss the month it needs; a direct read answers
   `200` with an empty array instead.
4. **A boundary day spans two archive months.** For any non-UTC Daily Coaching
   Timezone, the local previous day maps to a 24-hour UTC interval that can
   contain a month start, so the run reads **two** archives and merges them.
   The current feed
   (`services/coach-engine/src/profile_game_feed.rs:432`) walks the newest
   `MAX_CHESS_COM_ARCHIVE_MONTHS` from the list, which is discovery behavior,
   not window behavior.
5. **Filter by `end_time`, in UTC.** Eligibility is `end_time ∈ [local
   previous-day start, end)` converted to UTC. A Game that started on the day
   before the window and ended inside it is in the window; one that started
   inside and ended after is not.
6. **Archive outage is the real exposure.** A window is never carried forward
   (#222), so a
   provider-side archive gap costs those days silently, and V1 exposes no
   skipped-Game count. Whether that deserves an operator alert belongs to
   [Define Daily Coaching operational alerting and capacity thresholds](#301).

## Limitations

- 50 minutes of observation on one Sunday morning UTC. It does not cover peak
  European or US evening load, when an asynchronous write queue is most likely
  to lag.
- 44 appearances, one of them Daily. The Daily result is suggestive, not
  established.
- Lag is measured against `end_time` as the provider reports it. If the archive
  writer derived `end_time` at write time rather than at game end, measured lag
  would be systematically understated. The PGN `EndDate`/`EndTime` headers agree
  with `end_time` on every game inspected, which argues against that.
- No month-boundary appearance was observed live; the bucketing rule is
  established from stored data plus a cross-day case, not from watching a game
  finish at 00:00 UTC on the 1st.

## Sources

- [Chess.com Published-Data API](https://www.chess.com/news/view/published-data-api)
- [Published-Data API announcement](https://www.chess.com/announcements/view/published-data-api)
- [Game Archive Not Updating Right Away](https://www.chess.com/forum/view/general/game-archive-not-updating-right-away)
- [API (games archive) returns empty list of games](https://www.chess.com/clubs/forum/view/api-games-archive-returns-empty-list-of-games)
- [Published-Data API CHANGELOG](https://www.chess.com/clubs/forum/view/changelog)
