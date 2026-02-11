# Cockpit Report Schema

The `cockpit.report.v1` schema defines the aggregate report that cockpitctl produces.

## Schema Location

`schemas/cockpit.report.v1.json`

## Overview

The cockpit report is a "receipt of receipts": it summarizes all sensor results, applies policy, selects highlights, and records the policy snapshot used to compute the verdict.

## Top-Level Structure

```json
{
  "schema": "cockpit.report.v1",
  "tool": { ... },
  "run": { ... },
  "verdict": { ... },
  "sensors": [ ... ],
  "highlights": [ ... ],
  "policy": { ... },
  "data": { ... }
}
```

## Required Fields

### schema

```json
"schema": "cockpit.report.v1"
```

Must be exactly `"cockpit.report.v1"`.

### tool

cockpitctl version information.

```json
"tool": {
  "name": "cockpitctl",
  "version": "0.2.0",
  "commit": "abc1234"
}
```

### run

Execution context. Same structure as sensor reports.

```json
"run": {
  "started_at": "2024-01-15T10:35:00Z",
  "ended_at": "2024-01-15T10:35:01Z",
  "duration_ms": 1000
}
```

### verdict

The overall determination after applying policy.

```json
"verdict": {
  "status": "warn",
  "counts": {
    "info": 5,
    "warn": 2,
    "error": 0
  },
  "reasons": ["blocking_sensor_warned"]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `status` | enum | Overall verdict: `pass`, `warn`, `fail`, `skip` |
| `counts` | object | Aggregate finding counts across all sensors |
| `reasons` | array | Machine-readable reasons for the verdict |

### sensors

Per-sensor summaries.

```json
"sensors": [
  {
    "id": "builddiag",
    "blocking": true,
    "missing": "fail",
    "present": true,
    "verdict": {
      "status": "pass",
      "counts": { "info": 0, "warn": 0, "error": 0 }
    },
    "report_path": "artifacts/builddiag/report.json",
    "comment_path": "artifacts/builddiag/comment.md",
    "truncated": false,
    "errors": []
  }
]
```

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Sensor identifier |
| `blocking` | bool | Whether sensor is blocking per policy |
| `missing` | enum | Missing behavior: `skip`, `warn`, `fail` |
| `present` | bool | Whether receipt was found |
| `verdict` | object | Sensor's verdict (or synthesized if missing/invalid) |
| `report_path` | string | Path to sensor report |
| `comment_path` | string | Path to sensor comment (if present) |
| `truncated` | bool | Whether findings were capped |
| `errors` | array | Parse or validation errors |

### highlights

Top findings selected across all sensors.

```json
"highlights": [
  {
    "sensor_id": "diffguard",
    "finding": {
      "severity": "warn",
      "code": "diffguard.forbidden_api",
      "message": "Forbidden API introduced",
      "location": {
        "path": "src/lib.rs",
        "line": 42
      }
    }
  }
]
```

Each highlight includes:

| Field | Type | Description |
|-------|------|-------------|
| `sensor_id` | string | Which sensor produced this finding |
| `finding` | object | The finding itself (same structure as sensor findings) |

Highlights are:
- Deduplicated by fingerprint
- Sorted by severity (desc), blocking status, sensor_id, path, line, code
- Capped to `max_highlights`

### policy

Snapshot of the policy used to compute this verdict.

```json
"policy": {
  "warn_is_fail": false,
  "max_highlights": 7,
  "max_per_sensor_findings": 20,
  "max_annotations": 25,
  "section_order": ["Highlights", "Repo contract", "Other"],
  "sensors": [
    {
      "id": "builddiag",
      "blocking": true,
      "missing": "fail",
      "section": "Repo contract",
      "repro": "builddiag check"
    }
  ]
}
```

This captures the exact policy that produced the verdict, enabling reproducibility.

## Optional Fields

### data

Director-specific payload for dashboards or downstream tools.

```json
"data": {
  "total_sensors": 5,
  "blocking_passed": 3
}
```

## Example Complete Report

```json
{
  "schema": "cockpit.report.v1",
  "tool": {
    "name": "cockpitctl",
    "version": "0.2.0"
  },
  "run": {
    "started_at": "2024-01-15T10:35:00Z"
  },
  "verdict": {
    "status": "pass",
    "counts": {
      "info": 2,
      "warn": 0,
      "error": 0
    },
    "reasons": []
  },
  "sensors": [
    {
      "id": "builddiag",
      "blocking": true,
      "missing": "fail",
      "present": true,
      "verdict": {
        "status": "pass",
        "counts": { "info": 2, "warn": 0, "error": 0 }
      },
      "report_path": "artifacts/builddiag/report.json",
      "truncated": false,
      "errors": []
    }
  ],
  "highlights": [],
  "policy": {
    "warn_is_fail": false,
    "max_highlights": 7,
    "max_per_sensor_findings": 20,
    "max_annotations": 25,
    "section_order": ["Highlights", "Repo contract", "Other"],
    "sensors": [
      {
        "id": "builddiag",
        "blocking": true,
        "missing": "fail"
      }
    ]
  }
}
```

## See Also

- [Sensor Report Schema](sensor-report-schema.md) - Input format
- [Exit Codes](exit-codes.md) - How verdict maps to exit codes
