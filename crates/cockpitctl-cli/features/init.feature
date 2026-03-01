Feature: cockpitctl init command

  The init subcommand creates a starter cockpit.toml configuration
  file. It refuses to overwrite existing files to prevent accidental
  data loss.

  # ─────────────────────────────────────────────────────────────────────────────
  # Creating Configs
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: init creates a default config file
    Given a temporary directory
    When I run "cockpitctl init"
    Then the exit code is 0
    And the file "cockpit.toml" exists
    And the file "cockpit.toml" contains "[policy]"

  Scenario: init with custom path creates config at that path
    Given a temporary directory
    When I run "cockpitctl init --path custom.toml"
    Then the exit code is 0
    And the file "custom.toml" exists
    And the file "custom.toml" contains "[policy]"

  Scenario: init creates valid TOML with policy section
    Given a temporary directory
    When I run "cockpitctl init"
    Then the exit code is 0
    And the file "cockpit.toml" is valid TOML

  Scenario: init output contains sensor definitions
    Given a temporary directory
    When I run "cockpitctl init"
    Then the exit code is 0
    And the file "cockpit.toml" contains "[sensors"

  # ─────────────────────────────────────────────────────────────────────────────
  # Overwrite Protection
  # ─────────────────────────────────────────────────────────────────────────────

  Scenario: init refuses to overwrite existing config
    Given a temporary directory
    And a file "cockpit.toml" with content "# existing config"
    When I run "cockpitctl init"
    Then the exit code is 2
    And the file "cockpit.toml" contains "# existing config"

  Scenario: init refuses to overwrite at custom path
    Given a temporary directory
    And a file "custom.toml" with content "# my config"
    When I run "cockpitctl init --path custom.toml"
    Then the exit code is 2
    And the file "custom.toml" contains "# my config"

  Scenario: init twice fails on second attempt without corruption
    Given a temporary directory
    When I run "cockpitctl init"
    Then the exit code is 0
    When I run "cockpitctl init"
    Then the exit code is 2
    And the file "cockpit.toml" contains "[policy]"
