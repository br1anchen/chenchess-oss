---
status: accepted
---

# Separate identity, beta admission, and the Player dashboard

Renumbered from 0035 so that number stays with the Learning Plan selection
decision.

## Context

ADR 0032 assigned Firebase sign-in, verification, provider linking, and
invitation redemption to `/join`. That made the first beta implementation
coherent, but it also made `/join` both an identity boundary and an admission
workflow. A returning Player with Beta Access still entered a page framed
around joining the beta, while protected browser surfaces had to direct every
signed-out visitor to that blended journey.

The Central Host now needs an ordinary sign-in entry, an admission-only entry
for verified identities without Beta Access, and a Player home that can expose
the web product and private ChatGPT and Claude setup only after authorization.

## Decision

Split the browser journeys by responsibility:

- `/login/*` owns Firebase sign-in, signup, email verification, password
  reset, and explicit provider linking.
- `/join/*` owns Beta Access requests and invitation redemption. A signed-out
  or unverified visitor is returned to `/login`.
- `/dashboard/*` is the default destination for a verified Player with current
  Beta Access. It links the web coach and presents the authorized ChatGPT and
  Claude beta setup.
- `/app/*` and `/backoffice/*` send signed-out or unverified visitors to
  `/login`, not `/join`.

Firebase's managed email-action handler remains distinct from these browser
route owners. Verification and password-reset messages may open the shared
project's `chenchess.firebaseapp.com` handler, but each request carries the
originating Central Host's `/login/` URL as its continuation. This preserves
the staging or production identity journey without treating Firebase's
project-wide email-template callback as an environment-specific `authDomain`.

The public landing page no longer accepts an anonymous Beta Access Request.
Its product and beta actions enter `/login`; `/join` submits the request with
the current verified identity. The request API requires that Firebase identity
and derives the normalized request email from its verified claims; browser
email text is display-only and cannot select another account.

After a verified identity is established, Central Host checks Beta Access once
at the browser boundary. A current grant continues to the requested
allowlisted destination, defaulting to `/dashboard`. A missing grant continues
to the corresponding `/join` URL. An authenticated Review Session link retains
only its validated Review Session ID across those hops, and dashboard setup
links retain only the allowlisted `#chatgpt` or `#claude` target; arbitrary
return URLs and malformed identifiers are never carried forward. The back
office is the exception: its Firebase administrator claim is independent of
Player Beta Access, so a verified identity continues directly to `/backoffice`
and server-side administrator authorization remains authoritative.

Invitation email links remain `/join/#invite=...`. If `/join` must send the
visitor through `/login`, the captured invitation is carried only in the next
URL fragment. Each entry captures and scrubs the fragment synchronously. The
code never moves into a query string, log, or persistent browser store.

The dashboard, admission page, and back office each expose sign-out. Protected
pages still fail closed while identity or Beta Access cannot be confirmed.
Server-owned OAuth interactions begin their identity journey at `/login`; a
verified identity without Beta Access visits `/join` before returning to the
interaction. The interaction page submits the Firebase identity only after
that login/admission journey returns with an in-memory URL-fragment marker;
opening the interaction directly cannot skip its sign-in-first browser flow.

## Consequences

- Returning Players bypass beta-admission UI and land on a useful home.
- Identity and admission copy no longer compete on one page.
- Beta Access Requests have one browser caller and one verified email source.
- Public Web, ChatGPT, and Claude actions can consistently begin at `/login`
  while preserving only allowlisted return destinations.
- The route set gains two Vite/static entries, `/login` and `/dashboard`, but
  remains one Central Host deployment and one Firebase session.
- ADR 0032 remains the history of the original composition; this decision
  supersedes only its assignment of identity and redemption to one `/join`
  surface.
