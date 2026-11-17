use std::process::Command;

#[test]
fn example_config_runs_in_text_and_json_modes() {
    let binary = env!("CARGO_BIN_EXE_fairtime");
    let config = format!("{}/examples/global-team.toml", env!("CARGO_MANIFEST_DIR"));

    let text = Command::new(binary)
        .arg(&config)
        .output()
        .expect("text invocation should run");
    assert!(text.status.success());
    assert!(String::from_utf8_lossy(&text.stdout).contains("Global design review"));

    let json = Command::new(binary)
        .args([&config, "--format", "json"])
        .output()
        .expect("JSON invocation should run");
    assert!(json.status.success());
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(value["meetings"].as_array().unwrap().len(), 4);
}

#[test]
fn invalid_config_returns_a_useful_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bad.toml");
    std::fs::write(&path, "participants = []\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_fairtime"))
        .arg(path)
        .output()
        .expect("invalid invocation should run");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("at least one participant"));
}
