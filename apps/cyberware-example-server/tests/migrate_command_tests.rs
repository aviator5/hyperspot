#![allow(clippy::unwrap_used, clippy::expect_used)]

//! End-to-end tests for the `migrate` command
//!
//! These tests verify that the migrate CLI command works correctly
//! by invoking the cyberware-server binary and checking its output.

use std::process::Command;

/// Helper to get the path to the cyberware-server binary
fn cyberware_binary() -> String {
    std::env::var("CARGO_BIN_EXE_cyberware-example-server")
        .or_else(|_| std::env::var("CARGO_BIN_EXE_CYBERWARE_EXAMPLE_SERVER"))
        .expect("CARGO_BIN_EXE_cyberware-example-server must be set for tests")
}

#[test]
fn test_migrate_command_help_text() {
    let output = Command::new(cyberware_binary())
        .args(["migrate", "--help"])
        .output()
        .expect("failed to execute cyberware-server");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Run database migrations and exit"),
        "Help text should describe migrate command"
    );
}

#[test]
fn test_migrate_command_runs_migration_phases() {
    let output = Command::new(cyberware_binary())
        .arg("--config")
        .arg("../../config/e2e-local.yaml")
        .arg("migrate")
        .output()
        .expect("failed to execute cyberware-server");

    // Should complete successfully (with or without actual database)
    assert!(
        output.status.success(),
        "migrate command should exit successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
