Feature: Schema validation modes

  cockpitctl supports lax and strict schema validation modes.
  Lax mode skips JSON Schema validation; strict mode enforces it
  and surfaces violations as findings.

  Background:
    Given a clean output directory

  # ─────────────────────────────────────────────────────────────────────────────
  # Lax Mode
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: lax mode skips schema violations
    Given a fixture "schema_violation"
    When I run "cockpitctl ingest" on the fixture with "--schema-validation lax"
    Then the exit code is 0
    And the cockpit report does not contain a highlight "cockpit.schema_violation"
    And the cockpit report is valid JSON

  Scenario: lax mode still catches unparseable receipts
    Given a fixture "invalid_receipt"
    When I run "cockpitctl ingest" on the fixture with "--schema-validation lax"
    Then the exit code is 2
    And the cockpit report contains a highlight "cockpit.invalid_receipt"
    And the cockpit report is valid JSON

  Scenario: lax mode still catches oversized receipts
    Given a fixture "receipt_oversized"
    When I run "cockpitctl ingest" on the fixture with "--schema-validation lax"
    Then the exit code is 2
    And the cockpit report contains a highlight "cockpit.receipt_oversized"

  # ─────────────────────────────────────────────────────────────────────────────
  # Strict Mode
  # ─────────────────────────────────────────────────────────────────────────────

  @feature-schema
  Scenario: strict mode catches schema violations
    Given a fixture "schema_violation"
    When I run "cockpitctl ingest" on the fixture with "--schema-validation strict"
    Then the exit code is 2
    And the cockpit report contains a highlight "cockpit.schema_violation"
    And the cockpit report is valid JSON

  @feature-schema
  Scenario: strict mode passes valid receipts
    Given a fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture with "--schema-validation strict"
    Then the exit code is 0
    And the cockpit report is valid JSON
    And the report schema is "cockpit.report.v1"

  @feature-schema
  Scenario: strict mode passes empty findings receipt
    Given a fixture "empty_findings"
    When I run "cockpitctl ingest" on the fixture with "--schema-validation strict"
    Then the exit code is 0
    And the verdict status is "pass"

  # ─────────────────────────────────────────────────────────────────────────────
  # Validation Mode on Validate Subcommand
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: validate subcommand lax mode accepts valid receipt
    Given a temporary directory
    And a valid sensor report at "sensor.json"
    When I run "cockpitctl validate --input sensor.json --lax"
    Then the exit code is 0

  @feature-schema
  Scenario: validate subcommand strict mode accepts valid receipt
    Given a temporary directory
    And a valid sensor report at "sensor.json"
    When I run "cockpitctl validate --input sensor.json --strict"
    Then the exit code is 0

  @feature-schema
  Scenario: validate subcommand strict mode rejects incomplete receipt
    Given a temporary directory
    And a file "bad.json" with content "{\"tool\": {\"name\": \"x\", \"version\": \"1.0\"}}"
    When I run "cockpitctl validate --input bad.json --strict"
    Then the exit code is 1

  # ─────────────────────────────────────────────────────────────────────────────
  # Multiple Schema Violations in One Run
  # ─────────────────────────────────────────────────────────────────────────────

  @feature-schema
  Scenario: strict mode with multiple schema-violating sensors catches all
    Given a dynamic artifacts directory with sensors "bad1,bad2,good"
    And dynamic sensor "bad1" has a schema-violating receipt
    And dynamic sensor "bad2" has a schema-violating receipt
    And dynamic sensor "good" has verdict "pass"
    And a cockpit config with schema_validation "strict" and blocking sensors "bad1,bad2,good"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report is valid JSON
    And the cockpit report contains a highlight "cockpit.schema_violation"

  # ─────────────────────────────────────────────────────────────────────────────
  # Schema Violation Combined with Other Errors
  # ─────────────────────────────────────────────────────────────────────────────

  @feature-schema
  Scenario: strict mode schema violation combined with corrupt receipt
    Given a dynamic artifacts directory with sensors "schema_bad,corrupt,good"
    And dynamic sensor "schema_bad" has a schema-violating receipt
    And dynamic sensor "corrupt" has corrupt JSON content
    And dynamic sensor "good" has verdict "pass"
    And a cockpit config with schema_validation "strict" and blocking sensors "schema_bad,corrupt,good"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report is valid JSON
    And the cockpit report contains a highlight "cockpit.schema_violation"

  # ─────────────────────────────────────────────────────────────────────────────
  # Default Schema Validation Behavior
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: default schema validation mode is lax
    Given a dynamic artifacts directory with sensors "checksensor"
    And dynamic sensor "checksensor" has a schema-violating receipt
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the cockpit report does not contain a highlight "cockpit.schema_violation"
    And the cockpit report is valid JSON
