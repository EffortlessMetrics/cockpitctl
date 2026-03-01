Feature: Safety boundaries
  cockpitctl treats receipts as untrusted input and enforces
  strict safety controls on sensor IDs, file sizes, and counts.

  Background:
    Given a clean output directory

  # ─────────────────────────────────────────────────────────────────────────────
  # Path Traversal
  # ─────────────────────────────────────────────────────────────────────────────

  @new
  Scenario: Path traversal with dot-dot in sensor ID is rejected
    Given a dynamic artifacts directory with sensors "legit"
    And a raw sensor directory "../escape" with a valid receipt
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the sensors count is exactly 1

  @new
  Scenario: Absolute path sensor ID is rejected
    Given a dynamic artifacts directory with sensors "legit"
    And a raw sensor directory "/etc/passwd" with a valid receipt
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the sensors count is exactly 1

  @new
  Scenario: Backslash traversal in sensor ID is rejected
    Given a dynamic artifacts directory with sensors "legit"
    And a raw sensor directory "..\\escape" with a valid receipt
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the sensors count is exactly 1

  # ─────────────────────────────────────────────────────────────────────────────
  # Sensor Count Cap
  # ─────────────────────────────────────────────────────────────────────────────

  @new
  Scenario: Many sensors within default cap are all processed
    Given a dynamic artifacts directory with 10 sensors prefixed "s"
    And a cockpit config for all prefixed sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the sensors count is exactly 10
    And the sensors are in lexical order

  # ─────────────────────────────────────────────────────────────────────────────
  # Oversized Receipts
  # ─────────────────────────────────────────────────────────────────────────────

  @new
  Scenario: Oversized receipt produces a finding not a crash
    Given a fixture "receipt_oversized"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report is valid JSON
    And the cockpit report contains a highlight "cockpit.receipt_oversized"
    And the file "artifacts/cockpit/report.json" exists
    And the file "artifacts/cockpit/comment.md" exists

  # ─────────────────────────────────────────────────────────────────────────────
  # Multiple Safety Violations
  # ─────────────────────────────────────────────────────────────────────────────

  @new
  Scenario: Multiple hostile sensors are all rejected while legitimate ones pass
    Given a dynamic artifacts directory with sensors "good"
    And a raw sensor directory "../bad1" with a valid receipt
    And a raw sensor directory "../../bad2" with a valid receipt
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the sensors count is exactly 1
    And the cockpit report is valid JSON
