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

  # ─────────────────────────────────────────────────────────────────────────────
  # Input Validation
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: invalid receipt JSON is treated as a finding
    Given a fixture "invalid_receipt"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report contains a highlight "cockpit.invalid_receipt"

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
  # Determinism
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: output is deterministic across runs
    Given a fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture
    And I capture the report
    And I run "cockpitctl ingest" on the fixture again
    Then the reports are identical
