use super::*;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "mcpace-uninstall-test-{}-{}",
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
fn help_documents_preserved_state() {
    let mut stdout = Vec::new();
    let status = run(&["--help".to_string()], None, &mut stdout, &mut Vec::new());
    let output = String::from_utf8(stdout).unwrap();
    assert_eq!(status, 0);
    assert!(output.contains("Usage: mcpace uninstall"));
    assert!(output.contains("configuration"));
    assert!(output.contains("preserved"));
}

#[test]
fn dry_run_with_kept_clients_does_not_change_the_root() {
    let root = temp_root();
    let config = root.join("mcpace.config.json");
    let original = b"{\n  \"version\": \"0.8.2\",\n  \"servers\": {}\n}\n";
    fs::write(&config, original).unwrap();

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let status = run(
        &[
            "--dry-run".to_string(),
            "--keep-clients".to_string(),
            "--json".to_string(),
            "--root".to_string(),
            root.display().to_string(),
        ],
        None,
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(status, 0, "stderr: {}", String::from_utf8_lossy(&stderr));
    let report = parse_str(&String::from_utf8(stdout).unwrap()).unwrap();
    assert_eq!(
        report.get("schema").and_then(JsonValue::as_str),
        Some("mcpace.uninstall.v1")
    );
    assert_eq!(
        report.get("dryRun").and_then(JsonValue::as_bool),
        Some(true)
    );
    assert_eq!(fs::read(&config).unwrap(), original);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);

    let _ = fs::remove_dir_all(root);
}
