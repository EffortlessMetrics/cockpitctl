# Sensor Report Schema

The `sensor.report.v1` schema defines the envelope that sensors emit.

## Schema Location

`schemas/sensor.report.v1.json`

## Overview

A sensor receipt is an immutable record of what a sensor observed. cockpitctl only relies on envelope fields; tool-specific data lives in `data` fields and is treated as opaque.

## Top-Level Structure

```json
{
  "schema": "sensor.report.v1",
  "tool": { ... },
  "run": { ... },
  "verdict": { ... },
  "findings": [ ... ],
  "data": { ... }
}
```

## Required Fields

### schema

```json
"schema": "sensor.report.v1"
```

Schema identifier. Must be exactly `"sensor.report.v1"`.

### tool

Tool metadata.

```json
"tool": {
  "name": "builddiag",
  "version": "0.5.2",
  "commit": "abc1234"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Tool name |
| `version` | string | yes | Tool version |
| `commit` | string | no | Git commit of the tool |

### run

Execution context.

```json
"run": {
  "started_at": "2024-01-15T10:30:00Z",
  "ended_at": "2024-01-15T10:30:05Z",
  "duration_ms": 5000,
  "git": {
    "repo": "owner/repo",
    "head_sha": "abc1234",
    "base_sha": "def5678",
    "head_ref": "feature-branch",
    "base_ref": "main",
    "merge_base": "aaa1111"
  },
  "ci": {
    "provider": "github",
    "run_id": "12345",
    "run_url": "https://github.com/...",
    "job": "lint"
  },
  "host": {
    "os": "linux",
    "arch": "x86_64",
    "hostname": "runner-1"
  }
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `started_at` | datetime | yes | ISO 8601 timestamp |
| `ended_at` | datetime | no | ISO 8601 timestamp |
| `duration_ms` | int | no | Execution duration in milliseconds |
| `git` | object | no | Git context |
| `ci` | object | no | CI context |
| `host` | object | no | Host information |

### verdict

The sensor's determination.

```json
"verdict": {
  "status": "warn",
  "counts": {
    "info": 2,
    "warn": 1,
    "error": 0
  },
  "reasons": ["forbidden_api_detected"]
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `status` | enum | yes | One of: `pass`, `warn`, `fail`, `skip` |
| `counts` | object | yes | Finding counts by severity |
| `reasons` | array | no | Machine-readable reason codes |

**Status values:**

| Status | Meaning |
|--------|---------|
| `pass` | No issues found |
| `warn` | Advisory issues found |
| `fail` | Blocking issues found |
| `skip` | Sensor did not run (e.g., not applicable) |

### findings

Array of issues discovered.

```json
"findings": [
  {
    "severity": "warn",
    "code": "diffguard.forbidden_api",
    "message": "Forbidden API introduced in diff",
    "location": {
      "path": "src/lib.rs",
      "line": 42,
      "col": 5
    },
    "fingerprint": "sha256:abc123...",
    "check_id": "no-unsafe",
    "help": "See https://example.com/docs/no-unsafe",
    "url": "https://example.com/findings/123",
    "data": { "diff_hunk": "..." }
  }
]
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `severity` | enum | yes | One of: `info`, `warn`, `error` |
| `code` | string | yes | Stable finding code |
| `message` | string | yes | Human-readable description |
| `location` | object | no | File location |
| `fingerprint` | string | no | Stable identifier for deduplication |
| `check_id` | string | no | Rule/check identifier |
| `help` | string | no | Additional guidance |
| `url` | string | no | Link to more information |
| `data` | any | no | Tool-specific payload (opaque to cockpit) |

**Location object:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | no | Repo-relative file path |
| `line` | int | no | Line number (1-indexed) |
| `col` | int | no | Column number (1-indexed) |

## Optional Fields

### data

Tool-specific payload. cockpitctl treats this as opaque.

```json
"data": {
  "coverage_percent": 85.5,
  "uncovered_files": ["src/new.rs"]
}
```

## Extension Rules

1. Top-level fields are strict; unknown fields cause parse errors
2. Tool-specific data must go in `data` fields (report-level or finding-level)
3. Finding codes should be stable; never rename, only deprecate

## Example Complete Receipt

```json
{
  "schema": "sensor.report.v1",
  "tool": {
    "name": "builddiag",
    "version": "0.5.2"
  },
  "run": {
    "started_at": "2024-01-15T10:30:00Z",
    "duration_ms": 1234
  },
  "verdict": {
    "status": "pass",
    "counts": {
      "info": 0,
      "warn": 0,
      "error": 0
    }
  },
  "findings": []
}
```

## See Also

- [Cockpit Report Schema](cockpit-report-schema.md) - Aggregate output format
- [Write a Conformant Sensor](../how-to/write-conformant-sensor.md) - Authoring guide
