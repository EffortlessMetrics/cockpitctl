Feature: cockpitctl ingest

  The director ingests sensor receipts and produces one merge surface
  (cockpit.report.v1 + a PR comment) under strict budgets.

  Background:
    Given a clean output directory

  # ─────────────────────────────────────────────────────────────────────────────
  # Happy Path
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: happy path with a warning
    Given a fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the cockpit report matches the golden file
    And the cockpit comment matches the golden file
    And the verdict status is "warn"

  Scenario: empty findings still produces valid output
    Given a fixture "empty_findings"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "pass"
    And the highlights array is empty

  # ─────────────────────────────────────────────────────────────────────────────
  # Policy Failures
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: missing expected receipt fails the cockpit
    Given a fixture "missing_receipt"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the verdict status is "fail"
    And the cockpit report contains a highlight "cockpit.missing_receipt"

  Scenario: warn-as-fail policy promotes warnings to failures
    Given a fixture "warn_as_fail"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the verdict status is "fail"
    And the cockpit report contains a highlight "linter.unused_variable"

  # ─────────────────────────────────────────────────────────────────────────────
  # Deduplication and Ordering
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: duplicate fingerprints are deduplicated
    Given a fixture "duplicate_fingerprints"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report contains a highlight "dupescanner.duplicate_code"
    And the highlights are ordered by severity descending

  Scenario: highlights are capped to max_highlights policy limit
    Given a fixture "highlight_cap"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the verdict status is "fail"
    And the highlights count is exactly 3
    And the highlights are ordered by severity descending
    And the cockpit comment matches the golden file

  # ─────────────────────────────────────────────────────────────────────────────
  # Input Validation
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: invalid receipt JSON is treated as a finding
    Given a fixture "invalid_receipt"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report contains a highlight "cockpit.invalid_receipt"

  @feature-schema
  Scenario: schema violations in strict mode become findings
    Given a fixture "schema_violation"
    When I run "cockpitctl ingest" on the fixture with "--schema-validation strict"
    Then the exit code is 2
    And the cockpit report contains a highlight "cockpit.schema_violation"

  # ─────────────────────────────────────────────────────────────────────────────
  # Edge Cases
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: large messages are preserved in the report
    Given a fixture "large_messages"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report contains a highlight "verbosechk.complexity"
    And all finding messages are under 10000 characters

  Scenario: unicode paths are handled correctly
    Given a fixture "unicode_paths"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the cockpit report is valid JSON

  # ─────────────────────────────────────────────────────────────────────────────
  # Output Structure
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: report contains required schema field
    Given a fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture
    Then the report schema is "cockpit.report.v1"

  Scenario: report contains sensors summary
    Given a fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture
    Then the report contains sensors "builddiag" and "diffguard"

  Scenario: comment contains stable markers
    Given a fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture
    Then the comment contains "<!-- cockpit:begin -->"
    And the comment contains "<!-- cockpit:end -->"

  # ─────────────────────────────────────────────────────────────────────────────
  # Label-Gated Sensors
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: label-required sensor is skipped when label is missing
    Given a fixture "label_gated"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "pass"
    And the sensor "builddiag" has verdict status "pass"
    And the sensor "perftest" has verdict status "skip"
    And the highlights array is empty

  Scenario: label-required sensor processes when label is present
    Given a fixture "label_gated"
    When I run "cockpitctl ingest" on the fixture with "--label needs-perf-test"
    Then the exit code is 0
    And the verdict status is "warn"
    And the sensor "builddiag" has verdict status "pass"
    And the sensor "perftest" has verdict status "warn"
    And the cockpit report contains a highlight "perftest.regression"

  # ─────────────────────────────────────────────────────────────────────────────
  # Mixed Failure Modes
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: skip verdict sensor surfaces correctly
    Given a fixture "skip_receipt"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "pass"
    And the sensor "builddiag" has verdict status "pass"
    And the sensor "coverage" has verdict status "skip"
    And the highlights array is empty
    And the cockpit report matches the golden file
    And the cockpit comment matches the golden file

  Scenario: tool error with runtime failure finding
    Given a fixture "tool_error"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the verdict status is "fail"
    And the sensor "builddiag" has verdict status "pass"
    And the sensor "linter" has verdict status "fail"
    And the cockpit report contains a highlight "runtime_error"
    And the cockpit report matches the golden file
    And the cockpit comment matches the golden file

  Scenario: mixed verdicts with multiple failure modes
    Given a fixture "mixed_verdicts"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the verdict status is "fail"
    And the sensor "builddiag" has verdict status "pass"
    And the sensor "linter" has verdict status "fail"
    And the sensor "coverage" has verdict status "skip"
    And the sensor "perftest" has verdict status "warn"
    And the cockpit report contains a highlight "cockpit.invalid_receipt"
    And the cockpit report contains a highlight "cockpit.missing_receipt"
    And the cockpit report matches the golden file
    And the cockpit comment matches the golden file

  # ─────────────────────────────────────────────────────────────────────────────
  # Extended Feature Set (Roadmap Lock-In)
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: configurable max receipt size surfaces oversized receipts
    Given a fixture "receipt_oversized"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the verdict status is "fail"
    And the cockpit report contains a highlight "cockpit.receipt_oversized"

  Scenario: github annotations are emitted and capped by policy
    Given a fixture "annotation_cap"
    When I run "cockpitctl ingest" on the fixture with "--github-annotations"
    Then the exit code is 2
    And stdout contains "::error"
    And stdout has exactly 2 lines starting with "::"

  Scenario: sarif output is written when requested
    Given a fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture with "--format sarif"
    Then the exit code is 0
    And the file "artifacts/cockpit/sarif.json" exists
    And the JSON file "artifacts/cockpit/sarif.json" field "version" equals "2.1.0"

  Scenario: baseline trend output is rendered when baseline is provided
    Given a fixture "happy_path"
    And a baseline report from fixture "empty_findings"
    When I run "cockpitctl ingest" on the fixture with "--baseline {baseline_report}"
    Then the exit code is 0
    And stderr contains "### Trend"
    And stderr contains "new finding(s)"

  @feature-hooks
  Scenario: configured hooks append sections before sticky end marker
    Given a fixture "happy_path"
    And a hook script is configured
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the comment contains "### Hook Notes"
    And the comment contains "From hook"
    And in the comment "### Hook Notes" appears before "<!-- cockpit:end -->"

  @feature-buildfix
  Scenario: buildfix plans are surfaced in report data and comment
    Given a fixture "buildfix_plan"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the report data contains key "_buildfix"
    And the report field "data._buildfix.matched_count" equals 1
    And the report field "data._buildfix.unmatched_count" equals 1
    And the comment contains "### Buildfix"

  @feature-buildfix
  Scenario: buildfix auto-apply writes deterministic apply evidence
    Given a fixture "buildfix_plan"
    And a successful buildfix actuator script
    When I run "cockpitctl ingest" on the fixture with "--buildfix-auto-apply --buildfix-actuator {actuator_script} --buildfix-max-auto-apply-safety safe --buildfix-actuator-timeout-ms 5000"
    Then the exit code is 0
    And the file "artifacts/cockpit/buildfix.apply.json" exists
    And the JSON file "artifacts/cockpit/buildfix.apply.json" field "status" equals "applied"
    And the report data contains key "_buildfix_apply"
    And the report field "data._buildfix_apply.status" equals "applied"
    And the comment contains "### Buildfix Apply"

  @feature-policy-signing
  Scenario: policy signing writes signature evidence and comment section
    Given a fixture "happy_path"
    And a policy signing key file
    When I run "cockpitctl ingest" on the fixture with "--policy-sign --policy-sign-key-path {policy_sign_key} --policy-sign-key-id ci-key"
    Then the exit code is 0
    And the file "artifacts/cockpit/policy.signature.json" exists
    And the JSON file "artifacts/cockpit/policy.signature.json" field "schema" equals "cockpit.policy_signature.v1"
    And the report data contains key "_policy_signature"
    And the report field "data._policy_signature.key_id" equals "ci-key"
    And the comment contains "### Policy Signature"

  # --------------------------------------------------------------------------
  # Feature Gate Matrix
  # --------------------------------------------------------------------------

  @feature-hooks
  Scenario Outline: hook feature is runtime-gated
    Given a fixture "happy_path"
    And a hook script is configured
    When I run "cockpitctl ingest" on the fixture with "<args>"
    Then the exit code is 0
    And the feature "hooks" is "<state>"

    Examples:
      | args                                     | state   |
      | --format cockpit                         | present |
      | --disable-hooks --format cockpit         | absent  |

  @feature-buildfix
  Scenario Outline: buildfix feature is runtime-gated
    Given a fixture "buildfix_plan"
    When I run "cockpitctl ingest" on the fixture with "<args>"
    Then the exit code is 0
    And the feature "buildfix" is "<state>"

    Examples:
      | args                                        | state   |
      | --format cockpit                            | present |
      | --disable-buildfix --format cockpit         | absent  |

  @feature-policy-signing
  Scenario Outline: policy-signing feature is runtime-gated
    Given a fixture "happy_path"
    And a policy signing key file
    When I run "cockpitctl ingest" on the fixture with "<args>"
    Then the exit code is 0
    And the feature "policy-signing" is "<state>"

    Examples:
      | args                                                                                                         | state   |
      | --policy-sign --policy-sign-key-path {policy_sign_key} --policy-sign-key-id ci-key --format cockpit          | present |
      | --disable-policy-signing --policy-sign --policy-sign-key-path {policy_sign_key} --policy-sign-key-id ci-key --format cockpit | absent  |

  # ─────────────────────────────────────────────────────────────────────────────
  # Multi-Error Scenarios
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: multiple findings across severities are all surfaced
    Given a fixture "multi_error"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the verdict status is "fail"
    And the highlights count is exactly 3
    And the cockpit report contains a highlight "multierr.error"
    And the cockpit report contains a highlight "multierr.warn"
    And the cockpit report contains a highlight "multierr.note"
    And the highlights are ordered by severity descending

  # ─────────────────────────────────────────────────────────────────────────────
  # Safety Scenarios
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: hostile artifact pointers with path traversal are handled safely
    Given a fixture "hostile_pointers"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "pass"
    And the highlights array is empty
    And the cockpit report is valid JSON

  Scenario: oversized receipt surfaces a finding rather than crashing
    Given a fixture "receipt_oversized"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the verdict status is "fail"
    And the cockpit report contains a highlight "cockpit.receipt_oversized"
    And the cockpit report is valid JSON
    And the report schema is "cockpit.report.v1"

  # ─────────────────────────────────────────────────────────────────────────────
  # Cockpit Hints and Sensor Extensions
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: cockpit hints from sensor are reflected in highlights
    Given a fixture "cockpit_hints"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "warn"
    And the cockpit report contains a highlight "hintsensor.threshold"
    And the highlights count is exactly 1

  Scenario: reference sensor with rich metadata produces valid output
    Given a fixture "reference_sensor"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "warn"
    And the cockpit report contains a highlight "refsensor.complexity"
    And the cockpit report contains a highlight "refsensor.note"
    And the highlights count is exactly 2
    And the highlights are ordered by severity descending

  Scenario: artifact pointer sensor produces valid output
    Given a fixture "artifact_pointers"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "pass"
    And the highlights array is empty
    And the sensors count is exactly 1

  # ─────────────────────────────────────────────────────────────────────────────
  # Schema Validation Mode Scenarios
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: lax schema validation skips schema violations
    Given a fixture "schema_violation"
    When I run "cockpitctl ingest" on the fixture with "--schema-validation lax"
    Then the exit code is 0
    And the cockpit report does not contain a highlight "cockpit.schema_violation"
    And the cockpit report is valid JSON

  @feature-schema
  Scenario: strict schema validation catches schema violations
    Given a fixture "schema_violation"
    When I run "cockpitctl ingest" on the fixture with "--schema-validation strict"
    Then the exit code is 2
    And the cockpit report contains a highlight "cockpit.schema_violation"

  # ─────────────────────────────────────────────────────────────────────────────
  # Policy Evaluation Scenarios
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: warn-is-fail disabled allows warnings to pass
    Given a fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "warn"

  Scenario: label-gated sensor with multiple labels
    Given a fixture "label_gated"
    When I run "cockpitctl ingest" on the fixture with "--label some-other-label"
    Then the exit code is 0
    And the verdict status is "pass"
    And the sensor "perftest" has verdict status "skip"

  # ─────────────────────────────────────────────────────────────────────────────
  # Output Structure Scenarios
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: cockpit report always has cockpit.report.v1 schema
    Given a fixture "mixed_verdicts"
    When I run "cockpitctl ingest" on the fixture
    Then the report schema is "cockpit.report.v1"

  Scenario: cockpit report output files always exist even on failure
    Given a fixture "missing_receipt"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the file "artifacts/cockpit/report.json" exists
    And the file "artifacts/cockpit/comment.md" exists

  Scenario: comment always has stable begin and end markers
    Given a fixture "mixed_verdicts"
    When I run "cockpitctl ingest" on the fixture
    Then the comment contains "<!-- cockpit:begin -->"
    And the comment contains "<!-- cockpit:end -->"

  Scenario: multi-error report has correct sensors summary
    Given a fixture "multi_error"
    When I run "cockpitctl ingest" on the fixture
    Then the sensors count is exactly 1
    And the sensor "multierr" has verdict status "fail"

  # ─────────────────────────────────────────────────────────────────────────────
  # Determinism Scenarios
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: output is deterministic across runs
    Given a fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture
    And I capture the report
    And I run "cockpitctl ingest" on the fixture again
    Then the reports are identical

  Scenario: determinism with mixed verdicts across runs
    Given a fixture "mixed_verdicts"
    When I run "cockpitctl ingest" on the fixture
    And I capture the report
    And I run "cockpitctl ingest" on the fixture again
    Then the reports are identical

  Scenario: determinism with multi-error across runs
    Given a fixture "multi_error"
    When I run "cockpitctl ingest" on the fixture
    And I capture the report
    And I run "cockpitctl ingest" on the fixture again
    Then the reports are identical

  # ─────────────────────────────────────────────────────────────────────────────
  # Error Handling Scenarios
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: invalid receipt produces valid cockpit output
    Given a fixture "invalid_receipt"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report is valid JSON
    And the report schema is "cockpit.report.v1"
    And the file "artifacts/cockpit/comment.md" exists

  Scenario: missing receipt produces a finding with correct code
    Given a fixture "missing_receipt"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the verdict status is "fail"
    And the cockpit report contains a highlight "cockpit.missing_receipt"
    And the cockpit report is valid JSON

  Scenario: tool error sensor shows correct per-sensor verdict
    Given a fixture "tool_error"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the sensor "linter" has verdict status "fail"
    And the sensor "builddiag" has verdict status "pass"
    And the cockpit report is valid JSON
