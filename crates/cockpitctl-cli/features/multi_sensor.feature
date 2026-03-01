Feature: Multi-sensor ingest scenarios

  Tests for multi-sensor ingest behavior including mixed verdicts,
  deterministic ordering, highlight budgets, and comment coverage.

  Background:
    Given a clean output directory

  # ─────────────────────────────────────────────────────────────────────────────
  # All Passing
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: Three sensors all passing
    Given a dynamic artifacts directory with sensors "alpha,beta,gamma"
    And all dynamic sensors have verdict "pass"
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "pass"
    And the sensors count is exactly 3
    And the comment mentions sensors "alpha,beta,gamma"

  # ─────────────────────────────────────────────────────────────────────────────
  # Mixed Verdicts
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: Mixed verdicts with one blocking failure
    Given a dynamic artifacts directory with sensors "alpha,beta,gamma"
    And dynamic sensor "alpha" has verdict "pass"
    And dynamic sensor "beta" has verdict "fail" with finding "beta.critical_bug"
    And dynamic sensor "gamma" has verdict "warn" with finding "gamma.slow_test"
    And a cockpit config with blocking sensors "alpha,beta"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the verdict status is "fail"
    And the sensor "alpha" has verdict status "pass"
    And the sensor "beta" has verdict status "fail"
    And the sensor "gamma" has verdict status "warn"

  Scenario: Non-blocking failure does not cause policy failure
    Given a dynamic artifacts directory with sensors "alpha,beta"
    And dynamic sensor "alpha" has verdict "pass"
    And dynamic sensor "beta" has verdict "fail" with finding "beta.issue"
    And a cockpit config with blocking sensors "alpha"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the sensor "alpha" has verdict status "pass"
    And the sensor "beta" has verdict status "fail"

  # ─────────────────────────────────────────────────────────────────────────────
  # Deterministic Ordering
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: Sensors are discovered in lexical order
    Given a dynamic artifacts directory with sensors "zulu,alpha,mike"
    And all dynamic sensors have verdict "pass"
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the sensors are in lexical order

  # ─────────────────────────────────────────────────────────────────────────────
  # Highlight Budgets Across Sensors
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: Highlight budget is shared across sensors
    Given a dynamic artifacts directory with sensors "alpha,beta"
    And dynamic sensor "alpha" has verdict "fail" with 3 findings prefixed "alpha"
    And dynamic sensor "beta" has verdict "fail" with 3 findings prefixed "beta"
    And a cockpit config with max highlights 4 and all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the highlights count is at most 4
    And the highlights are ordered by severity descending

  # ─────────────────────────────────────────────────────────────────────────────
  # Determinism
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: Determinism with dynamic multi-sensor input
    Given a dynamic artifacts directory with sensors "alpha,beta,gamma"
    And dynamic sensor "alpha" has verdict "pass"
    And dynamic sensor "beta" has verdict "fail" with finding "beta.critical_bug"
    And dynamic sensor "gamma" has verdict "warn" with finding "gamma.slow_test"
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    And I capture the report
    And I run "cockpitctl ingest" on the fixture again
    Then the reports are identical

  # ─────────────────────────────────────────────────────────────────────────────
  # Output Structure
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: Multi-sensor report always has stable markers
    Given a dynamic artifacts directory with sensors "alpha,beta"
    And all dynamic sensors have verdict "pass"
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the comment contains "<!-- cockpit:begin -->"
    And the comment contains "<!-- cockpit:end -->"
    And the report schema is "cockpit.report.v1"

  Scenario: Multi-sensor outputs always written even on failure
    Given a dynamic artifacts directory with sensors "alpha,beta"
    And dynamic sensor "alpha" has verdict "pass"
    And dynamic sensor "beta" has verdict "fail" with finding "beta.bug"
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the file "artifacts/cockpit/report.json" exists
    And the file "artifacts/cockpit/comment.md" exists
