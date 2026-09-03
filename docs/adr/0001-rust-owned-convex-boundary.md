---
status: superseded by ADR-0002
---

# Review Engine-owned Convex boundary

ChenChess will keep Convex behind the Review Engine, then implemented by the Rust backend: the browser talks only to the Review Engine, and the Review Engine is the only Convex client. This is a deliberate deviation from Convex's common React-direct setup so Player authentication, future Saved Games authorization, Review Engine-owned browser WebSocket endpoints, and Game Review access stay behind one backend boundary. The Review Engine issues HTTP-only Player Session cookies, and `POST /api/analyze` must validate a Player Session before it can create a Game Review.

Superseded because the auth boundary moved to Better Auth SSO on the client, with the Review Engine validating the resulting Auth Token/JWT for protected backend endpoints.
