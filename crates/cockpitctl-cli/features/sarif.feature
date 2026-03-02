Feature: SARIF output
  cockpitctl --format sarif writes a SARIF v2.1.0 log alongside the standard
  cockpit report and comment.

  Background:
    Given a clean output directory

  Scenario: SARIF file is created with correct version
    Given a fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture with "--format sarif"
    Then the exit code is 0
    And the file "artifacts/cockpit/sarif.json" exists
    And the JSON file "artifacts/cockpit/sarif.json" field "version" equals "2.1.0"

  Scenario: SARIF output coexists with standard report and comment
    Given a fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture with "--format sarif"
    Then the exit code is 0
    And the file "artifacts/cockpit/report.json" exists
    And the file "artifacts/cockpit/comment.md" exists
    And the file "artifacts/cockpit/sarif.json" exists

  Scenario: SARIF contains schema field
    Given a fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture with "--format sarif"
    Then the exit code is 0
    And the JSON file "artifacts/cockpit/sarif.json" field "$schema" equals "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json"

  Scenario: SARIF output with multiple sensors
    Given a fixture "three_sensor_mixed"
    When I run "cockpitctl ingest" on the fixture with "--format sarif"
    Then the exit code is 2
    And the file "artifacts/cockpit/sarif.json" exists
    And the JSON file "artifacts/cockpit/sarif.json" field "version" equals "2.1.0"

  Scenario: SARIF output is deterministic
    Given a fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture with "--format sarif"
    And I capture the report
    And I run "cockpitctl ingest" on the fixture again
    Then the reports are identical

  Scenario: SARIF is not written without format flag
    Given a fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the file "artifacts/cockpit/sarif.json" does not exist
