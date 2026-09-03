---
status: accepted
---

# Serve the Landing Page separately from the web application

ADR 0032 refines this decision by composing these surfaces in the renamed
Central Host workspace and making `/app/*` the authenticated bootstrap.
ADR 0055 further separates Firebase identity at `/login`, Beta admission at
`/join`, and the authorized Player home at `/dashboard`; it also retires the
anonymous landing-page request form in favor of verified requests at `/join`.

The Central Host serves build-time static ChenChess HTML and CSS at `/`.
Build-time static Privacy, Terms, and Support pages live at `/privacy`,
`/terms`, and `/support`. The authenticated React coaching product lives at
`/app`, Firebase identity at `/login`, Beta admission and its authenticated
Beta Access Request form at `/join`, the authorized Player home at
`/dashboard`, and the Administrator-only Beta Back Office at `/backoffice`;
server-owned Coach OAuth interaction routes remain separate. Public Web,
ChatGPT, and Claude beta buttons first enter `/login`, and only a Player with
Beta Access continues to the requested product or host-specific setup. This
preserves the existing no-SSR decision while keeping public product copy
independent of Firebase initialization, authentication state,
coaching-application failures, and raw staging installation details.

The legal pages identify whoever runs the deployment as its service operator and data controller, with their own public contact. The pages carry reviewed terms and privacy disclosures rather than placeholders; a production release reviews the operator identity and legal text again.

The V1 public and authenticated surfaces include no marketing analytics, advertising pixels, session replay, or nonessential tracking cookies. They retain only essential authentication and session state plus minimized operational and security logs. A later request for product analytics requires a separate privacy and consent review rather than silently adding a tracker.

Every Central Host HTML entry links one origin-relative Web App Manifest and
the approved light and dark ChenChess app-icon variants. The manifest keeps
its identity, start URL, scope, and icon URLs origin-relative so the same
artifact is correct at both `staging.example` and `example.test`.
Favicons, install icons, Apple touch icons, and in-product app marks come from
the canonical `packages/ui/src/assets/brand/app-icons` set.

The staging privacy page discloses Firebase for identity, Google when the Player chooses Google sign-in, Resend for transactional invitation delivery and its standard 30-day sent-email retention, ImprovMX for inbound forwarding, Railway for application hosting, and Vercel for authoritative DNS. It distinguishes those processors and infrastructure providers from marketing recipients; waitlist data is never sold or used for advertising.

The privacy and support pages explain how to request beta-data deletion through `support@example.test`. The application retains access requests, invitations, delivery status, and Beta Access only until revocation, a verified deletion request, or beta closure; keyed rate-limit identifiers expire after 24 hours, and application security or operational logs expire within 30 days.

During staging, every beta page emits `noindex`, authentication and OAuth cookies are host-only to `staging.example`, and no automatic redirect is installed on `example.test`. The apex remains unchanged until the separately gated production release.
