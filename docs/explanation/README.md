# Explanation

Explanation documentation provides **conceptual understanding**. It discusses the "why" behind design decisions and helps you build a mental model of the system.

## Philosophy

### [Why cockpitctl](why-cockpitctl.md)

The problem space, design philosophy, and what cockpitctl deliberately doesn't do.

## Architecture

### [Hexagonal Architecture](hexagonal-architecture.md)

How the codebase is organized into microcrates with clean dependency boundaries.

### [Composition Model](composition-model.md)

How independent sensors compose into a unified cockpit.

## Design Decisions

### [Trust Boundaries](trust-boundaries.md)

The security model: why receipts are untrusted and what protections exist.

### [Determinism Design](determinism-design.md)

Why byte-stability matters and how it's achieved.

## Related

- [Reference](../reference/README.md) - For exact specifications
- [How-to Guides](../how-to/README.md) - For practical tasks
