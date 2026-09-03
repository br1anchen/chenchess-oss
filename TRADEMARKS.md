# Trademarks and brand assets

The code in this repository is licensed under the GNU Affero General Public
License v3.0 or later (see [`LICENSE`](LICENSE)). A copyright licence is not a
trademark licence, and the two are separated here on purpose: you may run,
modify and redistribute this software, and you may not present the result as
ChenChess.

## What the code licence does not cover

The following are **excluded from the AGPL grant** and reserved:

- The name **ChenChess**, and the wordmark set in the ChenChess brand kit.
- The **陳 surname seal** and every mark derived from it, including the knight
  mark.

None of these ship in this repository. The wordmark logos and the seal-derived
marks were removed rather than left unreferenced, and the application icons in
`packages/ui/src/assets/brand/app-icons/` are plain placeholder geometry that
carries neither. `packages/ui/src/assets.test.ts` asserts that no shipped asset
reintroduces them.

What remains reserved is the name itself: you may run, modify and redistribute
this software, and you may not present the result as ChenChess. Put your own
name in `BrandLockup` and your own artwork in `app-icons/`. Anything else —
including a fork that keeps the AGPL's section 13 source offer — is unaffected,
because the mark identifies who is running the service, not what the service
does.

Nominative reference stays fine: saying that your project is a fork of
ChenChess, or that it interoperates with it, needs no permission.

## What is granted, and how

The **board and brush textures** are granted separately under
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/), attributed to the
ChenChess project:

- `packages/ui/src/assets/brand/board/` — the dry-brush frame and the light and
  dark watercolour square textures.
- `packages/ui/src/assets/brand/brush/` — the brush strokes, ink blots and
  washes.
- `packages/ui/src/assets/brand/motion/` — the pigment, wash and watercolour
  control-frame masks.

They carry no seal geometry and no wordmark, so they are the part of the visual
system a fork can keep and build on.

The **chess pieces** in `packages/ui/src/assets/brand/chess-pieces/` and the
**coaching-value icons** in `packages/ui/src/assets/brand/icons/` are likewise
CC BY 4.0. They are drawings of chess pieces and of ideas, not identity.

## Third-party material inside the brand assets

`motion/watercolor-control-frame.svg` embeds an alpha silhouette cropped from
the CodyHouse "Ink Transition Effect" tutorial sprite; only the silhouette is
used, and every colour comes from component CSS. Vendored runtime notices are
in [`runtime/THIRD_PARTY_NOTICES.md`](runtime/THIRD_PARTY_NOTICES.md), and the
Maia-2 model licence is in
[`services/maia/licenses/`](services/maia/licenses/).

## Asking

Anything this document does not clearly permit, ask about first — open a
discussion rather than assuming. Contact details are in
[`CONTRIBUTING.md`](CONTRIBUTING.md).
