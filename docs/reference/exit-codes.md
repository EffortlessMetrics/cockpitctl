# Exit Codes

cockpitctl uses exit codes to communicate results to CI systems.

## Exit Code Reference

| Code | Name | Description |
|------|------|-------------|
| `0` | Pass | Overall verdict passes |
| `1` | Runtime Error | cockpitctl encountered an error |
| `2` | Policy Failure | Overall verdict fails due to policy |

## Detailed Semantics

### Exit Code 0 (Pass)

Returned when:
- All blocking sensors pass
- All blocking sensors warn, but `warn_is_fail = false`
- No blocking sensors are defined

The cockpit report and comment are written successfully.

### Exit Code 1 (Runtime Error)

Returned when cockpitctl itself fails:
- Cannot read required paths (artifacts directory, config file)
- Cannot write outputs
- Configuration file is malformed
- Internal error

cockpitctl may or may not write outputs on exit code 1, depending on when the error occurred.

### Exit Code 2 (Policy Failure)

Returned when:
- A blocking sensor has `verdict.status = "fail"`
- A blocking sensor has `verdict.status = "warn"` and `warn_is_fail = true`
- A blocking sensor is missing and its `missing` policy is `"fail"`

The cockpit report and comment are still written on exit code 2. This allows CI to post the comment showing what failed.

## Mapping from Verdict

| Overall Verdict | warn_is_fail | Exit Code |
|-----------------|--------------|-----------|
| `pass` | - | `0` |
| `warn` | `false` | `0` |
| `warn` | `true` | `2` |
| `fail` | - | `2` |
| `skip` | - | `0` |

## CI Integration

### GitHub Actions

```yaml
- name: Run cockpitctl
  id: cockpit
  continue-on-error: true
  run: cockpitctl ingest

- name: Post comment
  if: always()
  run: |
    # Post comment regardless of exit code
    gh pr comment --body-file artifacts/cockpit/comment.md

- name: Check result
  if: steps.cockpit.outcome == 'failure'
  run: exit 1
```

### Shell Scripts

```bash
cockpitctl ingest
code=$?

if [ $code -eq 1 ]; then
  echo "cockpitctl runtime error"
  exit 1
elif [ $code -eq 2 ]; then
  echo "Policy failure - see comment"
  # Still post comment
  exit 1
fi

echo "All checks passed"
```

## See Also

- [CLI Reference](cli.md) - Command documentation
- [Integrate with GitHub Actions](../how-to/integrate-github-actions.md) - CI setup guide
