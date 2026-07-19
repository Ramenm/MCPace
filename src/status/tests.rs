use super::*;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "mcpace-status-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn help_is_public_and_side_effect_free() {
    let mut stdout = Vec::new();
    let status = run(&["--help".to_string()], None, &mut stdout, &mut Vec::new());
    assert_eq!(status, 0);
    assert!(String::from_utf8(stdout)
        .unwrap()
        .contains("Usage: mcpace status"));
}

#[test]
fn status_reads_an_inactive_root_without_creating_state() {
    let root = temp_root();
    let config = root.join("mcpace.config.json");
    let original = b"{\n  \"version\": \"0.8.2\",\n  \"servers\": {}\n}\n";
    fs::write(&config, original).unwrap();

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let status = run(
        &[
            "--json".to_string(),
            "--root".to_string(),
            root.display().to_string(),
        ],
        None,
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(status, 1, "stderr: {}", String::from_utf8_lossy(&stderr));
    let report = parse_str(&String::from_utf8(stdout).unwrap()).unwrap();
    assert_eq!(
        report.get("schema").and_then(JsonValue::as_str),
        Some("mcpace.status.v1")
    );
    assert_eq!(fs::read(&config).unwrap(), original);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);

    let _ = fs::remove_dir_all(root);
}
