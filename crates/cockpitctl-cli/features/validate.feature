Feature: cockpitctl validate command

  The validate subcommand checks whether a JSON file is a valid
  sensor receipt or cockpit report. It supports --strict and --lax
  modes and produces clear error messages.

  # ─────────────────────────────────────────────────────────────────────────────
  # Valid Receipts
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: validate accepts a valid sensor receipt in lax mode
    Given a temporary directory
    And a valid sensor report at "sensor.json"
    When I run "cockpitctl validate --input sensor.json --lax"
    Then the exit code is 0
    And stderr contains "ok"

  Scenario: validate accepts a valid cockpit report in lax mode
    Given a temporary directory
    And a valid cockpit report at "cockpit.json"
    When I run "cockpitctl validate --input cockpit.json --lax"
    Then the exit code is 0
    And stderr contains "ok"

  @feature-schema
  Scenario: validate accepts a valid sensor receipt in strict mode
    Given a temporary directory
    And a valid sensor report at "sensor.json"
    When I run "cockpitctl validate --input sensor.json --strict"
    Then the exit code is 0
    And stderr contains "ok"

  @feature-schema
  Scenario: validate accepts a valid cockpit report in strict mode
    Given a temporary directory
    And a valid cockpit report at "cockpit.json"
    When I run "cockpitctl validate --input cockpit.json --strict"
    Then the exit code is 0

  # ─────────────────────────────────────────────────────────────────────────────
  # Invalid Receipts
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: validate rejects malformed JSON
    Given a temporary directory
    And a file "bad.json" with content "{ not valid json }"
    When I run "cockpitctl validate --input bad.json --lax"
    Then the exit code is 1

  Scenario: validate rejects truncated JSON
    Given a temporary directory
    And a file "truncated.json" with content "{\"schema\": \"sensor.report.v1\""
    When I run "cockpitctl validate --input truncated.json --lax"
    Then the exit code is 1

  Scenario: validate rejects JSON missing required fields
    Given a temporary directory
    And a file "incomplete.json" with content "{\"tool\": {\"name\": \"x\", \"version\": \"1.0\"}}"
    When I run "cockpitctl validate --input incomplete.json --lax"
    Then the exit code is 1

  Scenario: validate rejects empty file
    Given a temporary directory
    And a file "empty.json" with content ""
    When I run "cockpitctl validate --input empty.json --lax"
    Then the exit code is 1

  # ─────────────────────────────────────────────────────────────────────────────
  # Missing File
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: validate reports useful error for missing file
    Given a temporary directory
    When I run "cockpitctl validate --input nonexistent.json --lax"
    Then the exit code is 1
    And stderr contains "nonexistent.json"

  Scenario: validate reports useful error for missing file in strict mode
    Given a temporary directory
    When I run "cockpitctl validate --input nonexistent.json --strict"
    Then the exit code is 1
    And stderr contains "nonexistent.json"

  # ─────────────────────────────────────────────────────────────────────────────
  # Fixture-Based Validation
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: validate accepts a receipt with empty findings
    Given a fixture "empty_findings"
    When I run "cockpitctl validate" with input "artifacts/emptycheck/report.json"
    Then the exit code is 0

  Scenario: validate rejects a malformed receipt from fixture
    Given a fixture "invalid_receipt"
    When I run "cockpitctl validate" with input "artifacts/malformed/report.json"
    Then the exit code is 1
