---
status: partially superseded by ADR-0023
---

# Vite and Rust central hosting

ChenChess will support a centrally hosted deployment using a Railway-style multi-service layout, but will keep the frontend as a Vite-built React app rather than adopting Next.js. The central host runs the Rust backend, the static Vite frontend, and required engine/model services; the Railway Rust/React starter is a deployment reference, not a framework decision. For the Central Host MVP, Stockfish runs inside the Rust backend container while Maia runs as a separate service behind the Human Move Model adapter.
