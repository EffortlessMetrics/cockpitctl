# cockpitctl-sarif

SARIF v2.1.0 export for `cockpit.report.v1` findings.

## Scope
- Converts cockpit highlights into SARIF results.
- Builds deterministic SARIF rule and fingerprint structures.
- Produces either structured SARIF types or pretty JSON output.

## Key exports
- `cockpit_report_to_sarif`
- `cockpit_report_to_sarif_json`
