Feature: Error handling

  cockpitctl must handle error conditions gracefully, producing
  valid output and useful diagnostics rather than crashing.

  Background:
    Given a clean output directory

  # ─────────────────────────────────────────────────────────────────────────────
  # Missing Artifacts Directory
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: ingest with nonexistent artifacts directory still produces output
    Given a temporary directory
    And a minimal cockpit config
    When I run "cockpitctl ingest --artifacts nonexistent_dir --config cockpit.toml"
    Then the exit code is 0

  Scenario: ingest with missing config uses defaults gracefully
    Given a temporary directory
    When I run "cockpitctl ingest --artifacts . --config nonexistent.toml"
    Then the exit code is 0

  # ─────────────────────────────────────────────────────────────────────────────
  # Corrupt Receipts
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: corrupt receipt produces valid cockpit output with finding
    Given a fixture "invalid_receipt"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report is valid JSON
    And the report schema is "cockpit.report.v1"
    And the cockpit report contains a highlight "cockpit.invalid_receipt"
    And the file "artifacts/cockpit/comment.md" exists

  Scenario: oversized receipt produces a finding rather than crashing
    Given a fixture "receipt_oversized"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report is valid JSON
    And the report schema is "cockpit.report.v1"
    And the cockpit report contains a highlight "cockpit.receipt_oversized"
    And the file "artifacts/cockpit/comment.md" exists

  # ─────────────────────────────────────────────────────────────────────────────
  # Path Traversal Attempts
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: hostile path traversal in sensor IDs is handled safely
    Given a fixture "hostile_pointers"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the cockpit report is valid JSON
    And the report schema is "cockpit.report.v1"
    And the highlights array is empty

  # ─────────────────────────────────────────────────────────────────────────────
  # Missing Expected Receipts
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: missing expected receipt produces finding not crash
    Given a fixture "missing_receipt"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report is valid JSON
    And the cockpit report contains a highlight "cockpit.missing_receipt"
    And the verdict status is "fail"
    And the file "artifacts/cockpit/comment.md" exists

  Scenario: tool error sensor does not crash the pipeline
    Given a fixture "tool_error"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report is valid JSON
    And the sensor "linter" has verdict status "fail"
    And the sensor "builddiag" has verdict status "pass"
    And the file "artifacts/cockpit/report.json" exists
    And the file "artifacts/cockpit/comment.md" exists

  # ─────────────────────────────────────────────────────────────────────────────
  # Empty and Minimal Artifacts
  # ─────────────────────────────────────────────────────────────────────────────

  @new
  Scenario: empty artifacts directory produces valid output with no sensors
    Given a temporary directory
    And a minimal cockpit config
    And an empty artifacts subdirectory
    When I run "cockpitctl ingest --artifacts artifacts --config cockpit.toml"
    Then the exit code is 0
    And the file "artifacts/cockpit/report.json" exists
    And the file "artifacts/cockpit/comment.md" exists

  @new
  Scenario: sensor directory with no report.json is treated as missing receipt
    Given a dynamic artifacts directory with sensors "alpha"
    And the file "artifacts/alpha/report.json" is deleted
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report is valid JSON
    And the cockpit report contains a highlight "cockpit.missing_receipt"

  # ─────────────────────────────────────────────────────────────────────────────
  # Multiple Corrupt Receipts
  # ─────────────────────────────────────────────────────────────────────────────

  @new
  Scenario: multiple corrupt receipts all produce individual findings
    Given a dynamic artifacts directory with sensors "bad1,bad2,good"
    And dynamic sensor "bad1" has corrupt JSON content
    And dynamic sensor "bad2" has corrupt JSON content
    And dynamic sensor "good" has verdict "pass"
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the cockpit report is valid JSON
    And the cockpit report contains a highlight "cockpit.invalid_receipt"
    And the file "artifacts/cockpit/report.json" exists

  # ─────────────────────────────────────────────────────────────────────────────
  # Truncated and Minimal JSON
  # ─────────────────────────────────────────────────────────────────────────────

  @new
  Scenario: truncated JSON receipt produces a finding
    Given a dynamic artifacts directory with sensors "trunc"
    And dynamic sensor "trunc" has truncated JSON content
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the cockpit report is valid JSON
    And the cockpit report contains a highlight "cockpit.invalid_receipt"

  @new
  Scenario: receipt with empty JSON object produces a finding
    Given a dynamic artifacts directory with sensors "emptyobj"
    And dynamic sensor "emptyobj" has content "{}"
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the cockpit report is valid JSON
    And the cockpit report contains a highlight "cockpit.invalid_receipt"

  # ─────────────────────────────────────────────────────────────────────────────
  # Config Edge Cases
  # ─────────────────────────────────────────────────────────────────────────────

  @new
  Scenario: empty config file uses defaults gracefully
    Given a temporary directory
    And a file "cockpit.toml" with content ""
    And an empty artifacts subdirectory
    When I run "cockpitctl ingest --artifacts artifacts --config cockpit.toml"
    Then the exit code is 0
    And the file "artifacts/cockpit/report.json" exists
    And the file "artifacts/cockpit/comment.md" exists

  @new
  Scenario: config with only policy section and no sensors works
    Given a temporary directory
    And a minimal cockpit config
    And an empty artifacts subdirectory
    When I run "cockpitctl ingest --artifacts artifacts --config cockpit.toml"
    Then the exit code is 0
    And the file "artifacts/cockpit/report.json" exists
    And the file "artifacts/cockpit/comment.md" exists

  @new
  Scenario: sensors in artifacts but not in config are still discovered
    Given a dynamic artifacts directory with sensors "discovered"
    And dynamic sensor "discovered" has verdict "pass"
    And a minimal cockpit config
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the cockpit report is valid JSON
    And the sensors count is at most 1

  # ─────────────────────────────────────────────────────────────────────────────
  # Receipt Content Edge Cases
  # ─────────────────────────────────────────────────────────────────────────────

  @new
  Scenario: receipt with JSON array instead of object produces a finding
    Given a dynamic artifacts directory with sensors "arrayreceipt"
    And dynamic sensor "arrayreceipt" has content "[1, 2, 3]"
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the cockpit report is valid JSON
    And the cockpit report contains a highlight "cockpit.invalid_receipt"

  @new
  Scenario: receipt with wrong schema field produces a finding
    Given a dynamic artifacts directory with sensors "wrongschema"
    And dynamic sensor "wrongschema" has content "{\"schema\": \"unknown.schema.v99\", \"tool\": {\"name\": \"x\", \"version\": \"1.0\"}, \"findings\": []}"
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the cockpit report is valid JSON
    And the cockpit report contains a highlight "cockpit.invalid_receipt"

  @new
  Scenario: receipt with null bytes in content produces a finding
    Given a dynamic artifacts directory with sensors "nullbytes"
    And dynamic sensor "nullbytes" has corrupt JSON content
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the cockpit report is valid JSON
    And the cockpit report contains a highlight "cockpit.invalid_receipt"
