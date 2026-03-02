Feature: Explain command
  cockpitctl explain shows human-readable explanations for cockpit finding codes.

  Scenario: explain shows details for missing_receipt code
    Given a temporary directory
    When I run "cockpitctl explain cockpit.missing_receipt"
    Then the exit code is 0
    And stdout contains "missing_receipt"

  Scenario: explain shows details for invalid_receipt code
    Given a temporary directory
    When I run "cockpitctl explain cockpit.invalid_receipt"
    Then the exit code is 0
    And stdout contains "invalid_receipt"

  Scenario: explain shows details for schema_violation code
    Given a temporary directory
    When I run "cockpitctl explain cockpit.schema_violation"
    Then the exit code is 0
    And stdout contains "schema_violation"

  Scenario: explain shows details for receipt_oversized code
    Given a temporary directory
    When I run "cockpitctl explain cockpit.receipt_oversized"
    Then the exit code is 0
    And stdout contains "receipt_oversized"

  Scenario: explain all lists known cockpit codes
    Given a temporary directory
    When I run "cockpitctl explain all"
    Then the exit code is 0
    And stdout contains "cockpit.missing_receipt"
    And stdout contains "cockpit.invalid_receipt"

  Scenario: explain returns non-zero for unknown code
    Given a temporary directory
    When I run "cockpitctl explain cockpit.no_such_code"
    Then the exit code is 1
    And stderr contains "unknown"

  Scenario: explain returns non-zero for unknown prefix
    Given a temporary directory
    When I run "cockpitctl explain unknown.prefix.code"
    Then the exit code is 1
    And stderr contains "unknown"

