Feature: Policy evaluation edge cases

  The cockpit policy engine must correctly evaluate verdicts across
  a range of sensor outcomes: all passing, all failing, mixed, and
  various warn-is-fail configurations.

  Background:
    Given a clean output directory

  # ─────────────────────────────────────────────────────────────────────────────
  # All Pass
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: all sensors passing yields overall pass
    Given a fixture "empty_findings"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "pass"
    And the highlights array is empty
    And the cockpit report is valid JSON

  Scenario: all sensors passing with happy path yields warn (has findings)
    Given a fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "warn"
    And the cockpit report is valid JSON

  # ─────────────────────────────────────────────────────────────────────────────
  # All Fail
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: missing receipt causes overall policy failure
    Given a fixture "missing_receipt"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the verdict status is "fail"
    And the cockpit report contains a highlight "cockpit.missing_receipt"

  Scenario: warn-as-fail makes warnings into failures
    Given a fixture "warn_as_fail"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the verdict status is "fail"

  # ─────────────────────────────────────────────────────────────────────────────
  # Mixed Verdicts
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: mixed verdicts picks the worst overall status
    Given a fixture "mixed_verdicts"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the verdict status is "fail"
    And the sensor "builddiag" has verdict status "pass"
    And the sensor "linter" has verdict status "fail"
    And the sensor "coverage" has verdict status "skip"
    And the sensor "perftest" has verdict status "warn"

  Scenario: skip sensor does not affect overall passing verdict
    Given a fixture "skip_receipt"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "pass"
    And the sensor "builddiag" has verdict status "pass"
    And the sensor "coverage" has verdict status "skip"
    And the highlights array is empty

  # ─────────────────────────────────────────────────────────────────────────────
  # Label Gating
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: label-gated sensor is skipped without matching label
    Given a fixture "label_gated"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "pass"
    And the sensor "perftest" has verdict status "skip"

  Scenario: label-gated sensor activates with matching label
    Given a fixture "label_gated"
    When I run "cockpitctl ingest" on the fixture with "--label needs-perf-test"
    Then the exit code is 0
    And the verdict status is "warn"
    And the sensor "perftest" has verdict status "warn"

  Scenario: non-matching label still skips gated sensor
    Given a fixture "label_gated"
    When I run "cockpitctl ingest" on the fixture with "--label unrelated-label"
    Then the exit code is 0
    And the verdict status is "pass"
    And the sensor "perftest" has verdict status "skip"

  # ─────────────────────────────────────────────────────────────────────────────
  # Highlight Capping
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: highlights are capped to policy max_highlights
    Given a fixture "highlight_cap"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the highlights count is exactly 3
    And the highlights are ordered by severity descending

  # ─────────────────────────────────────────────────────────────────────────────
  # Deduplication
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: duplicate findings are deduplicated
    Given a fixture "duplicate_fingerprints"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the highlights are ordered by severity descending

  # ─────────────────────────────────────────────────────────────────────────────
  # Output Invariants
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: cockpit report always written even on policy failure
    Given a fixture "missing_receipt"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the file "artifacts/cockpit/report.json" exists
    And the file "artifacts/cockpit/comment.md" exists
    And the report schema is "cockpit.report.v1"

  Scenario: comment always has stable markers regardless of verdict
    Given a fixture "mixed_verdicts"
    When I run "cockpitctl ingest" on the fixture
    Then the comment contains "<!-- cockpit:begin -->"
    And the comment contains "<!-- cockpit:end -->"

  # ─────────────────────────────────────────────────────────────────────────────
  # Exit Code Semantics — Verdict Combinations
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: all pass sensors yield exit code 0
    Given a dynamic artifacts directory with sensors "a,b,c"
    And all dynamic sensors have verdict "pass"
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "pass"

  Scenario: all skip sensors yield exit code 0
    Given a dynamic artifacts directory with sensors "a,b"
    And dynamic sensor "a" has verdict "skip"
    And dynamic sensor "b" has verdict "skip"
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the highlights array is empty

  Scenario: only warn sensors without warn_is_fail yield exit code 0
    Given a dynamic artifacts directory with sensors "w1,w2"
    And dynamic sensor "w1" has verdict "warn" with finding "w1.minor"
    And dynamic sensor "w2" has verdict "warn" with finding "w2.minor"
    And a cockpit config with warn_is_fail false and blocking sensors "w1,w2"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "warn"

  Scenario: only warn sensors with warn_is_fail yield exit code 2
    Given a dynamic artifacts directory with sensors "w1,w2"
    And dynamic sensor "w1" has verdict "warn" with finding "w1.minor"
    And dynamic sensor "w2" has verdict "warn" with finding "w2.minor"
    And a cockpit config with warn_is_fail true and blocking sensors "w1,w2"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the verdict status is "fail"

  Scenario: pass and skip combined yield exit code 0
    Given a dynamic artifacts directory with sensors "passer,skipper"
    And dynamic sensor "passer" has verdict "pass"
    And dynamic sensor "skipper" has verdict "skip"
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the verdict status is "pass"
    And the sensor "passer" has verdict status "pass"
    And the sensor "skipper" has verdict status "skip"

  Scenario: blocking fail with pass and skip yields exit code 2
    Given a dynamic artifacts directory with sensors "good,bad,skipped"
    And dynamic sensor "good" has verdict "pass"
    And dynamic sensor "bad" has verdict "fail" with finding "bad.error"
    And dynamic sensor "skipped" has verdict "skip"
    And a cockpit config with all sensors blocking
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 2
    And the verdict status is "fail"
    And the sensor "good" has verdict status "pass"
    And the sensor "bad" has verdict status "fail"
    And the sensor "skipped" has verdict status "skip"

  Scenario: non-blocking fail with all other pass yields exit code 0
    Given a dynamic artifacts directory with sensors "core,addon"
    And dynamic sensor "core" has verdict "pass"
    And dynamic sensor "addon" has verdict "fail" with finding "addon.issue"
    And a cockpit config with blocking sensors "core"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the sensor "core" has verdict status "pass"
    And the sensor "addon" has verdict status "fail"
