# How-to Guides

How-to guides are **task-oriented** recipes that show you how to accomplish specific goals. They assume you already understand the basics.

## CI Integration

### [Integrate with GitHub Actions](integrate-github-actions.md)

Set up a complete CI workflow that runs sensors, invokes cockpitctl, and posts PR comments.

### [Handle Missing Receipts](handle-missing-receipts.md)

Configure what happens when an expected sensor doesn't produce a receipt.

## Customization

### [Customize the PR Comment](customize-pr-comment.md)

Control section order, highlight count, and per-sensor display.

### [Debug Failing Ingest](debug-failing-ingest.md)

Troubleshoot common issues with receipt parsing and policy evaluation.

## Validation

### [Validate Receipts](validate-receipts.md)

Use the `validate` command to check receipt and report structure.

### [Smoke Test a Release](smoke-test-release.md)

Validate a release using only published artifacts (no vendoring required).

## Sensor Development

### [Write a Conformant Sensor](write-conformant-sensor.md)

Build a sensor that emits valid `sensor.report.v1` receipts.

### [Test Sensor Conformance](test-sensor-conformance.md)

Verify your sensor produces correct, deterministic output.

### [Sensor Authoring Checklist](sensor-authoring-checklist.md)

P0 / P1 / P2 prioritized checklist for building conformant sensors.

## Related

- [Tutorials](../tutorials/README.md) - If you're just getting started
- [Reference](../reference/README.md) - For exact specifications
- [Explanation](../explanation/README.md) - For conceptual background
