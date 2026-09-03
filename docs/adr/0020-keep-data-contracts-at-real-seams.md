# Keep data contracts at real seams

Status note: ADR 0023 supersedes the no-schema-version rule only for
Firestore-backed Review Session Checkpoints. The remaining real-seam,
product-state, evaluation-artifact, and naming decisions stay in force.
ADR 0026 applies those boundaries to intent by keeping enrichment inputs
transient and retaining final commentary only as ordinary review output.

ChenChess keeps explicit contracts for Rust/web/Coach wire messages, tamper-evident imported Games, canonical chess Positions, runtime installation metadata, and durable Central Host retention data. Product operations return product state only; comment authoring provenance and provider recordings are evaluation concerns and are persisted only inside durable evaluation artifacts that need them. Internal traces, baselines, generated reports, and transient state are regenerated with the code and carry no schema version or generic Snapshot/Record/Manifest wrapper. Durable artifacts have one version at their top level, reproduction data uses direct typed inputs instead of nested manifests, Selector Experiment Runs contain their search configuration directly, and “manifest” is reserved for the runtime release descriptor. This supersedes the Snapshot/Record/Manifest model in ADR 0018 and the product-facing reproduction-manifest clauses in ADRs 0017 and 0019.
