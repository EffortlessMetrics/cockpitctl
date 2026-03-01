# cockpitctl-io-policy-signing

Policy-signing key loading adapter boundary extracted from `cockpitctl-io`.

## Scope

- Loads policy signing key bytes from a file path or environment variable.
- Applies resolution order: `key_path` (file) takes precedence over `key_env`.
- Strips trailing newlines and rejects empty key material.

## Architecture

This crate belongs to the **I/O adapter layer**. It bridges the domain signing
logic to the filesystem and process environment for key material retrieval.

## Key exports

- `load_policy_signing_key` — load key bytes from config, returning `Option<Vec<u8>>`.

## Usage

```rust
use cockpitctl_io_policy_signing::load_policy_signing_key;
use cockpitctl_types::PolicySigningConfig;

let config = PolicySigningConfig {
    enabled: true,
    key_path: Some("/path/to/key".into()),
    ..Default::default()
};
if let Some(key) = load_policy_signing_key(&config)? {
    println!("key loaded ({} bytes)", key.len());
}
```

## Further reading

See the [cockpitctl repository](https://github.com/cockpitctl/cockpitctl) for full
documentation and architecture overview.

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT License](../../LICENSE-MIT), at your option.
