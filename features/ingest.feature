Feature: cockpitctl ingest

  The director ingests sensor receipts and produces one merge surface
  (cockpit.report.v1 + a PR comment) under strict budgets.

  Scenario: happy path with a warning
    Given a fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the cockpit report matches the golden file
    And the cockpit comment matches the golden file

  Scenario: missing expected receipt fails the cockpit
    Given a fixture "missing_receipt"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report contains a highlight "cockpit.missing_receipt"

  Scenario: warn-as-fail policy promotes warnings to failures
    Given a fixture "warn_as_fail"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report contains a highlight "linter.unused_variable"

  Scenario: duplicate fingerprints are deduplicated
    Given a fixture "duplicate_fingerprints"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report contains a highlight "dupescanner.duplicate_code"

  Scenario: large messages are preserved in the report
    Given a fixture "large_messages"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report contains a highlight "verbosechk.complexity"

  Scenario: empty findings still produces valid output
    Given a fixture "empty_findings"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0

  Scenario: invalid receipt JSON is treated as a finding
    Given a fixture "invalid_receipt"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report contains a highlight "cockpit.invalid_receipt"
