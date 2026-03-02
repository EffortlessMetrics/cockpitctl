@feature-buildfix
Feature: Buildfix integration
  cockpitctl surfaces buildfix plans and auto-apply evidence when the feature
  is enabled.

  Background:
    Given a clean output directory

  Scenario: Buildfix plan data is present in report
    Given a fixture "buildfix_plan"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the report data contains key "_buildfix"

  Scenario: Buildfix matched and unmatched counts
    Given a fixture "buildfix_plan"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the report field "data._buildfix.matched_count" equals 1
    And the report field "data._buildfix.unmatched_count" equals 1

  Scenario: Buildfix section appears in comment
    Given a fixture "buildfix_plan"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the comment contains "### Buildfix"

  Scenario: Buildfix is suppressed when disabled
    Given a fixture "buildfix_plan"
    When I run "cockpitctl ingest" on the fixture with "--disable-buildfix"
    Then the exit code is 0
    And the comment does not contain "### Buildfix"
    And the feature "buildfix" is "absent"

  Scenario: Buildfix report output is deterministic
    Given a fixture "buildfix_plan"
    When I run "cockpitctl ingest" on the fixture
    And I capture the report
    And I run "cockpitctl ingest" on the fixture again
    Then the reports are identical

  Scenario: Buildfix coexists with normal cockpit outputs
    Given a fixture "buildfix_plan"
    When I run "cockpitctl ingest" on the fixture
    Then the exit code is 0
    And the file "artifacts/cockpit/report.json" exists
    And the file "artifacts/cockpit/comment.md" exists
    And the cockpit report is valid JSON
    And the report schema is "cockpit.report.v1"
