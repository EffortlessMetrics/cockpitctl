Feature: cockpitctl CLI commands

  The cockpitctl CLI provides commands beyond ingest for
  initializing configs and validating receipts.

  # ─────────────────────────────────────────────────────────────────────────────
  # Init Command
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: init creates a starter config
    Given a temporary directory
    When I run "cockpitctl init"
    Then the exit code is 0
    And the file "cockpit.toml" exists
    And the file "cockpit.toml" contains "[policy]"

  Scenario: init refuses to overwrite existing config
    Given a temporary directory
    And a file "cockpit.toml" with content "existing"
    When I run "cockpitctl init"
    Then the exit code is 2
    And the file "cockpit.toml" contains "existing"

  # ─────────────────────────────────────────────────────────────────────────────
  # Validate Command
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: validate passes for valid sensor report
    Given a fixture "happy_path"
    When I run "cockpitctl validate" with input "artifacts/builddiag/report.json"
    Then the exit code is 0

  Scenario: validate fails for invalid JSON
    Given a fixture "invalid_receipt"
    When I run "cockpitctl validate" with input "artifacts/malformed/report.json"
    Then the exit code is 1

  # ─────────────────────────────────────────────────────────────────────────────
  # Explain Command
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: explain shows details for a known cockpit code
    Given a temporary directory
    When I run "cockpitctl explain cockpit.missing_receipt"
    Then the exit code is 0
    And stdout contains "Missing Receipt"
    And stdout contains "A sensor declared in cockpit.toml did not produce a receipt file."

  Scenario: explain all lists known cockpit codes
    Given a temporary directory
    When I run "cockpitctl explain all"
    Then the exit code is 0
    And stdout contains "cockpit.missing_receipt"
    And stdout contains "cockpit.receipt_oversized"

  Scenario: explain returns non-zero for unknown code
    Given a temporary directory
    When I run "cockpitctl explain cockpit.no_such_code"
    Then the exit code is 1
    And stderr contains "unknown code: cockpit.no_such_code"

  # ─────────────────────────────────────────────────────────────────────────────
  # Validate Command — Additional Scenarios
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: validate passes for a reference sensor receipt
    Given a fixture "reference_sensor"
    When I run "cockpitctl validate" with input "artifacts/refsensor/report.json"
    Then the exit code is 0

  Scenario: validate passes for empty findings receipt
    Given a fixture "empty_findings"
    When I run "cockpitctl validate" with input "artifacts/emptycheck/report.json"
    Then the exit code is 0

  # ─────────────────────────────────────────────────────────────────────────────
  # Explain Command — Additional Scenarios
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: explain shows details for invalid receipt code
    Given a temporary directory
    When I run "cockpitctl explain cockpit.invalid_receipt"
    Then the exit code is 0
    And stdout contains "Invalid Receipt"

  Scenario: explain shows details for schema violation code
    Given a temporary directory
    When I run "cockpitctl explain cockpit.schema_violation"
    Then the exit code is 0
