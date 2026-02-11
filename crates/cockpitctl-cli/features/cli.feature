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
