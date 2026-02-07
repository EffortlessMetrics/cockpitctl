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

## Documentation Structure

```
docs/
├── tutorials/          Learning-oriented, hands-on lessons
├── how-to/             Task-oriented practical guides
├── reference/          Technical specifications
├── explanation/        Conceptual understanding
└── archive/            Historical documents

contracts/docs/
├── tokens.md           Canonical reason token registry
└── identity-spec.md    Vocabulary and fingerprint rules
```
