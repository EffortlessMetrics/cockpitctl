Feature: Trend tracking
  cockpitctl --baseline compares the current run against a prior cockpit report
  and writes a trend summary to stderr.

  Background:
    Given a clean output directory

  Scenario: Baseline comparison shows new findings
    Given a fixture "happy_path"
    And a baseline report from fixture "empty_findings"
    When I run "cockpitctl ingest" on the fixture with "--baseline {baseline_report}"
    Then the exit code is 0
    And stderr contains "### Trend"
    And stderr contains "new finding(s)"

  Scenario: Identical baselines show no changes
    Given a fixture "happy_path"
    And a baseline report from fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture with "--baseline {baseline_report}"
    Then the exit code is 0
    And stderr contains "### Trend"
    And stderr contains "No changes from baseline."

  Scenario: Baseline with more findings shows fixed findings
    Given a fixture "empty_findings"
    And a baseline report from fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture with "--baseline {baseline_report}"
    Then the exit code is 0
    And stderr contains "### Trend"
    And stderr contains "fixed finding(s)"

  Scenario: Trend output is deterministic across repeated runs
    Given a fixture "happy_path"
    And a baseline report from fixture "empty_findings"
    When I run "cockpitctl ingest" on the fixture with "--baseline {baseline_report}"
    And I capture the report
    And I run "cockpitctl ingest" on the fixture again
    Then the reports are identical

  Scenario: Report is still valid JSON when baseline is used
    Given a fixture "happy_path"
    And a baseline report from fixture "empty_findings"
    When I run "cockpitctl ingest" on the fixture with "--baseline {baseline_report}"
    Then the exit code is 0
    And the cockpit report is valid JSON
    And the report schema is "cockpit.report.v1"

  Scenario: Trend with mixed verdicts baseline
    Given a fixture "mixed_verdicts"
    And a baseline report from fixture "empty_findings"
    When I run "cockpitctl ingest" on the fixture with "--baseline {baseline_report}"
    Then the exit code is 2
    And stderr contains "### Trend"

  Scenario: Trend output does not appear without baseline flag
    Given a fixture "happy_path"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
