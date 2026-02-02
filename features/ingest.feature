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
