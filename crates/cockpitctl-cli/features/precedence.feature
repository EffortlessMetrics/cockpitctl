Feature: Precedence contract
  Config provides defaults; CLI flags override only when explicitly
  provided. This ensures the three-layer precedence: built-in defaults
  → cockpit.toml → CLI flags.

  Background:
    Given a clean output directory

  # ─────────────────────────────────────────────────────────────────────────────
  # Schema Validation Precedence
  # ─────────────────────────────────────────────────────────────────────────────

  @new @feature-schema
  Scenario: Config schema_validation=strict applies when CLI flag absent
    Given a dynamic artifacts directory with sensors "checksensor"
    And dynamic sensor "checksensor" has a schema-violating receipt
    And a cockpit config with schema_validation "strict" and blocking sensors "checksensor"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the cockpit report contains a highlight "cockpit.schema_violation"

  @new @feature-schema
  Scenario: CLI --schema-validation lax overrides config strict
    Given a dynamic artifacts directory with sensors "checksensor"
    And dynamic sensor "checksensor" has a schema-violating receipt
    And a cockpit config with schema_validation "strict" and blocking sensors "checksensor"
    When I run "cockpitctl ingest" on the fixture with "--schema-validation lax"
    Then the exit code is 0
    And the cockpit report does not contain a highlight "cockpit.schema_violation"

  @new @feature-schema
  Scenario: CLI --schema-validation strict overrides config lax
    Given a dynamic artifacts directory with sensors "checksensor"
    And dynamic sensor "checksensor" has a schema-violating receipt
    And a cockpit config with schema_validation "lax" and blocking sensors "checksensor"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the cockpit report does not contain a highlight "cockpit.schema_violation"
    When I run "cockpitctl ingest" on the fixture with "--schema-validation strict"
    Then the exit code is 2
    And the cockpit report contains a highlight "cockpit.schema_violation"

  # ─────────────────────────────────────────────────────────────────────────────
  # Warn-is-Fail Precedence
  # ─────────────────────────────────────────────────────────────────────────────

  @new
  Scenario: Config warn_is_fail=false allows warnings to pass
    Given a dynamic artifacts directory with sensors "checker"
    And dynamic sensor "checker" has verdict "warn" with finding "checker.minor"
    And a cockpit config with warn_is_fail false and blocking sensors "checker"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "warn"

  @new
  Scenario: Config warn_is_fail=true promotes warnings to failures
    Given a dynamic artifacts directory with sensors "checker"
    And dynamic sensor "checker" has verdict "warn" with finding "checker.minor"
    And a cockpit config with warn_is_fail true and blocking sensors "checker"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the verdict status is "fail"

  # ─────────────────────────────────────────────────────────────────────────────
  # Max Highlights Precedence
  # ─────────────────────────────────────────────────────────────────────────────

  @new
  Scenario: Config max_highlights caps the output
    Given a dynamic artifacts directory with sensors "noisysensor"
    And dynamic sensor "noisysensor" has verdict "fail" with 10 findings prefixed "noisysensor"
    And a cockpit config with max highlights 3 and all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the highlights count is at most 3
    And the highlights are ordered by severity descending

  # ─────────────────────────────────────────────────────────────────────────────
  # Config and CLI Combined Overrides
  # ─────────────────────────────────────────────────────────────────────────────

  @new
  Scenario: Config warn_is_fail=true with a pass sensor still passes
    Given a dynamic artifacts directory with sensors "clean"
    And dynamic sensor "clean" has verdict "pass"
    And a cockpit config with warn_is_fail true and blocking sensors "clean"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "pass"

  @new
  Scenario: Config max_highlights=1 with multiple findings caps to one
    Given a dynamic artifacts directory with sensors "loud"
    And dynamic sensor "loud" has verdict "fail" with 5 findings prefixed "loud"
    And a cockpit config with max highlights 1 and all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the highlights count is at most 1
