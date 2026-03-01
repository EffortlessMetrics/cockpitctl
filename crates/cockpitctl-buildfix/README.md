# cockpitctl-buildfix

Pure buildfix matching logic extracted from domain for SRP boundaries:

- Match buildfix plan entries to cockpit findings/highlights.
- Deterministically sort fix summaries.
- Select auto-apply candidates under safety and matched-finding gates.
