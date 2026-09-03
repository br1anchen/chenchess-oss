# Product chrome

Visual target: `docs/design/brand/chenchess-workspace-application-target.jpg`.
The watercolor primitives live in `@chenchess/ui/components/watercolor`.

## Subtitles

Do not add a subtitle, eyebrow, kicker, or supporting line on any surface —
cards, forms, page sections, and heroes alike — unless the user explicitly
asked for that copy in the request.

That includes `WatercolorCard` / `WatercolorNotice` `eyebrow`, uppercase
letter-spaced section eyebrows, invented invitation-only labels, and extra
sentences under a title that only restate the heading. If the title is
enough, stop there. This applies to marketing surfaces (the landing page) the
same as product chrome.

Existing chrome the user already approved (for example the AuthStudio
"Private beta" lockup) stays until they ask to change it. Do not grow it.

## Success and failure

Player-visible success and failure on product chrome use a watercolor
primitive — `WatercolorNotice` or `AuthNotice` — never Astryx `Banner`.
Raw Astryx is for layout stacks, previews, backoffice, and the foundation
check.

## Sign-in is not invitation-gated

Anyone may create an account and verify their email, then request Beta
Access. Do not write copy that says sign-in or sign-up requires an
invitation. Invitation redemption is a later, optional path.
