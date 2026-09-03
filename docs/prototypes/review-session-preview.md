# Review Session preview

The `/preview/web/review-session` placeholder was removed on 2026-08-25. The
chrome guidance below still holds for `apps/central-host/src/review-session/`.

The live Review Session and `/preview/web/review-session` share the watercolor
kit.

GitHub context:

- Wayfinder map: [Design Lichess-native interactive game review](#24)
- Watercolor / ChenChess surfaces: [Rebrand every web-facing surface onto the autopilot positioning as ChenChess](#362)

## Chrome to use

- Cards, notices, and fields: `WatercolorCard`, `WatercolorNotice`,
  `WatercolorField`
- Ply control: shared `WatercolorMoveNav` (web review, this preview, Coach App
  move-sequence)
- Tokens: the ink-wash theme's Astryx names (`--color-*`, `--radius-*`,
  `--shadow-*`) plus the board palette in `chessTokens.css`
- Paper fills stay opaque ivory (0.96–0.97). Ink sits on the brush-frame
  border. No washi, no instrument wash.

The preview route mounts `DashboardPreviewPlaceholder` plus
`LandingReviewSessionShowcase`. Both must keep kit nav under the board.

## Accepted direction (behavior)

These interaction rules still hold. They describe the production session, not a
deleted local study.

- Conversation-first layout: a large graphical board beside one continuous
  Critical Moment conversation.
- The board is the primary move control. At the Critical Moment, ranked arrows
  show the best, better, and good alternatives. Only each destination square is
  clickable.
- One move and evaluation panel navigates the real game until the Player
  selects an alternative, then that Stockfish branch. Exit returns to the real
  Critical Moment.
- The real-game evaluation graph stays fixed during branch exploration. Show
  the projected branch evaluation only on the board evaluation bar.
- The full real-game move sequence marks every Critical Moment with its kind
  glyph. Do not add commentary for every move in v1.
- Opening Identification stays compact and above the board. Related Practice is
  a separate optional lesson card.
- The intent read is inline, at the end of the comment paragraph. The Player
  corrects it in their own words. There is no separate hypothesis card and no
  fixed Yes/No/Skip choices.
- Positive Highlights and Improvement Opportunities are peers: same card, same
  ordering, same affordances. Kind is an explicit label and glyph.

## Run locally

From the repository root:

```sh
bun run dev:central-host
```

Open `/preview/web/review-session` on that preview origin.
