# cockpitctl Documentation

`cockpitctl` is a **director** that aggregates sensor receipts into a single merge decision and PR comment.

This documentation follows the [Diataxis framework](https://diataxis.fr/).

## Quick Links

| If you want to... | Go to... |
|---|---|
| Learn cockpitctl from scratch | [Tutorials](tutorials/README.md) |
| Accomplish a specific task | [How-to Guides](how-to/README.md) |
| Look up technical details | [Reference](reference/README.md) |
| Understand concepts and design | [Explanation](explanation/README.md) |

## By Audience

### CI/DevOps Engineers

Setting up cockpitctl in your pipeline:

1. [Getting Started](tutorials/getting-started.md) - First run with sample data
2. [Integrate with GitHub Actions](how-to/integrate-github-actions.md) - Complete CI workflow
3. [CLI Reference](reference/cli.md) - Commands and flags
4. [Config Reference](reference/config.md) - cockpit.toml settings

### Sensor Authors

Building tools that emit receipts:

1. [Write a Conformant Sensor](how-to/write-conformant-sensor.md) - Sensor authoring guide
2. [Sensor Report Schema](reference/sensor-report-schema.md) - Receipt envelope spec
3. [Test Sensor Conformance](how-to/test-sensor-conformance.md) - Validation and conformance harness
4. [Token Registry](../contracts/docs/tokens.md) - Reason tokens and identity tuples
5. [Composition Model](explanation/composition-model.md) - How sensors work together

### Maintainers

Understanding the system:

1. [Why cockpitctl](explanation/why-cockpitctl.md) - Problem space and philosophy
2. [Hexagonal Architecture](explanation/hexagonal-architecture.md) - Code organization
3. [Trust Boundaries](explanation/trust-boundaries.md) - Security model
4. [Determinism Design](explanation/determinism-design.md) - Why byte-stability matters
5. [Identity Specification](../contracts/docs/identity-spec.md) - Vocabulary and fingerprint rules
6. [Compatibility Promise](reference/compatibility.md) - SemVer and contract stability

### Release Managers

Managing releases:

1. [Release Preparation Guide](how-to/release-preparation-guide.md) - Comprehensive release process guide
2. [Release Manager Checklist](how-to/release-manager-checklist.md) - Detailed release checklist
3. [Release Runbook](how-to/release-runbook.md) - Step-by-step release execution
4. [Smoke Test a Release](how-to/smoke-test-release.md) - Post-release validation
5. [Verification](VERIFICATION.md) - README badge meanings, generated endpoints, and PR evidence boundaries
6. [Release-Ready Gate Checklist](../RELEASE_READY_GATE_CHECKLIST.md) - Workflow verification

## Documentation Structure

```
docs/
├── tutorials/          Learning-oriented, hands-on lessons
├── how-to/             Task-oriented practical guides
├── reference/          Technical specifications
├── explanation/        Conceptual understanding
└── archive/            Historical documents

contracts/docs/
├── tokens.md                   Canonical reason token registry
├── identity-spec.md            Vocabulary and fingerprint rules
├── artifact-layout.md          Directory structure and file layout
├── comment-abi.md              PR comment contract and stability
├── determinism.md              Determinism requirements
└── presence-and-missingness.md Presence semantics and missing receipts
```
