---
status: superseded by ADR-0023
---

# Better Auth SSO with Review Engine token validation

ChenChess will use the Convex Better Auth component for SSO and token issuance, while the Review Engine, implemented by the Rust backend, protects application endpoints by validating the client-provided Auth Token/JWT. The browser sends this token to the Review Engine as `Authorization: Bearer <jwt>` on protected requests, and the Review Engine treats the JWT `sub` claim as the canonical Player ID. This replaces backend-owned email/password proxying and HTTP-only Player Session cookies because the client should handle SSO UX, and the Review Engine should only trust verified tokens when creating Game Reviews or later accessing Saved Games.
