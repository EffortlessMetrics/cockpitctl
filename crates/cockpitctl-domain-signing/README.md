# cockpitctl-domain-signing

Policy signing domain boundary extracted from `cockpitctl-domain`.

## Scope

- Computes canonical (compact JSON) bytes for a `PolicySnapshot`.
- Derives SHA-256 digest of the policy snapshot.
- Signs the snapshot with HMAC-SHA256 and produces `PolicySignatureEvidence`.

## Architecture

This crate belongs to the **domain layer**. It contains pure cryptographic logic
with no filesystem, network, or CLI dependencies.

## Key exports

- `canonical_policy_snapshot_bytes` — deterministic JSON serialization.
- `policy_snapshot_sha256_hex` — SHA-256 hex digest of the policy.
- `sign_policy_snapshot` — sign with the configured algorithm.
- `sign_policy_snapshot_hmac_sha256` — HMAC-SHA256 signing.

## Usage

```rust
use cockpitctl_domain_signing::sign_policy_snapshot;
use cockpitctl_types::{PolicySignatureAlgorithm, PolicySnapshot};

let evidence = sign_policy_snapshot(
    &policy,
    PolicySignatureAlgorithm::HmacSha256,
    b"secret-key",
    Some("key-v1".into()),
).unwrap();
assert_eq!(evidence.signature.len(), 64);
```

## Further reading

See the [cockpitctl repository](https://github.com/cockpitctl/cockpitctl) for full
documentation and architecture overview.

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT License](../../LICENSE-MIT), at your option.
