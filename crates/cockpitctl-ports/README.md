# cockpitctl-ports

Hexagonal boundary contracts for cockpit ingestion:

- Port traits: `ReceiptSource`, `PolicySource`, `OutputSink`, `SchemaValidator`
- Boundary DTOs: `DiscoveredSensors`, `ReportRead`, `CommentRead`, `PlanRead`
- Schema helper types: `SchemaValidationResult`, `NoOpSchemaValidator`

This crate exists to keep orchestration (`cockpitctl-ingest`) focused on the ingest
use-case while adapters (`cockpitctl-io`) implement contracts in a separate,
SRP-aligned microcrate.
