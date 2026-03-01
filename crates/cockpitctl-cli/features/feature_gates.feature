Feature: Feature-gated behaviors
  As a CI pipeline operator
  I want cockpitctl to respect feature flags
  So that optional behaviors can be enabled/disabled independently

  Background:
    Given a clean output directory

  # ─────────────────────────────────────────────────────────────────────────────
  # Hooks Feature Gate
  # ─────────────────────────────────────────────────────────────────────────────

  @feature-hooks
  Scenario: Disabling hooks preserves normal ingest output
    Given a fixture "happy_path"
    And a hook script is configured
    When I run "cockpitctl ingest" on the fixture with "--disable-hooks"
    Then the exit code is 0
    And the cockpit report is valid JSON
    And the report schema is "cockpit.report.v1"
    And the comment contains "<!-- cockpit:begin -->"
    And the comment contains "<!-- cockpit:end -->"
    And the comment does not contain "### Hook Notes"

  @feature-hooks
  Scenario: Disabling hooks does not affect verdict calculation
    Given a fixture "happy_path"
    And a hook script is configured
    When I run "cockpitctl ingest" on the fixture with "--disable-hooks"
    Then the exit code is 0
    And the verdict status is "warn"
    And the report contains sensors "builddiag" and "diffguard"

  # ─────────────────────────────────────────────────────────────────────────────
  # Buildfix Feature Gate
  # ─────────────────────────────────────────────────────────────────────────────

  @feature-buildfix
  Scenario: Disabling buildfix suppresses buildfix data in report
    Given a fixture "buildfix_plan"
    When I run "cockpitctl ingest" on the fixture with "--disable-buildfix"
    Then the exit code is 0
    And the cockpit report is valid JSON
    And the report schema is "cockpit.report.v1"
    And the feature "buildfix" is "absent"

  @feature-buildfix
  Scenario: Disabling buildfix preserves normal comment markers
    Given a fixture "buildfix_plan"
    When I run "cockpitctl ingest" on the fixture with "--disable-buildfix"
    Then the exit code is 0
    And the comment contains "<!-- cockpit:begin -->"
    And the comment contains "<!-- cockpit:end -->"
    And the comment does not contain "### Buildfix"

  # ─────────────────────────────────────────────────────────────────────────────
  # Policy Signing Feature Gate
  # ─────────────────────────────────────────────────────────────────────────────

  @feature-policy-signing
  Scenario: Disabling policy signing suppresses signature artifacts
    Given a fixture "happy_path"
    And a policy signing key file
    When I run "cockpitctl ingest" on the fixture with "--disable-policy-signing --policy-sign --policy-sign-key-path {policy_sign_key} --policy-sign-key-id ci-key"
    Then the exit code is 0
    And the cockpit report is valid JSON
    And the feature "policy-signing" is "absent"

  @feature-policy-signing
  Scenario: Disabling policy signing preserves verdict and comment
    Given a fixture "happy_path"
    And a policy signing key file
    When I run "cockpitctl ingest" on the fixture with "--disable-policy-signing --policy-sign --policy-sign-key-path {policy_sign_key} --policy-sign-key-id ci-key"
    Then the exit code is 0
    And the verdict status is "warn"
    And the comment contains "<!-- cockpit:begin -->"
    And the comment contains "<!-- cockpit:end -->"
    And the comment does not contain "### Policy Signature"

  # ─────────────────────────────────────────────────────────────────────────────
  # Combined Feature Disable
  # ─────────────────────────────────────────────────────────────────────────────

  @feature-hooks @feature-buildfix
  Scenario: Disabling hooks and buildfix together preserves core output
    Given a fixture "buildfix_plan"
    And a hook script is configured
    When I run "cockpitctl ingest" on the fixture with "--disable-hooks --disable-buildfix"
    Then the exit code is 0
    And the cockpit report is valid JSON
    And the report schema is "cockpit.report.v1"
    And the comment contains "<!-- cockpit:begin -->"
    And the comment contains "<!-- cockpit:end -->"
    And the comment does not contain "### Hook Notes"
    And the comment does not contain "### Buildfix"
    And the feature "hooks" is "absent"
    And the feature "buildfix" is "absent"

  # ─────────────────────────────────────────────────────────────────────────────
  # Feature Determinism
  # ─────────────────────────────────────────────────────────────────────────────

  @feature-hooks
  Scenario: Determinism is preserved when hooks are disabled
    Given a fixture "happy_path"
    And a hook script is configured
    When I run "cockpitctl ingest" on the fixture with "--disable-hooks"
    And I capture the report
    And I run "cockpitctl ingest" on the fixture again
    Then the reports are identical
