# Package boundaries

The live rule lives in `CODING_STANDARDS.md`.

`apps/` and `services/` source files must not mention each other's trees, and
`packages/` must not import `apps/`. The gate that asserted this also asserted
the deployment topology, and went with it.
