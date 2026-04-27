mod common;

use common::*;
use std::fs;

#[test]
fn lab_coverage_json_reports_surface_classes_and_constraints() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::create_dir_all(root.join("eval").join("fixtures").join("runtime")).unwrap();
    fs::write(
        root.join("eval").join("runtime-capabilities.json"),
        r#"{
  "version": "0.3.5",
  "features": [
    {
      "id": "planner-client-surface-catalog",
      "area": "planner",
      "title": "Client surface catalog",
      "status": "implemented",
      "priority": "p0",
      "summary": "done",
      "evidence": ["src/client_catalog.rs"],
      "nextStep": "keep it current"
    },
    {
      "id": "planner-surface-constraint-warnings",
      "area": "planner",
      "title": "Surface constraint warnings",
      "status": "implemented",
      "priority": "p0",
      "summary": "done",
      "evidence": ["src/client.rs"],
      "nextStep": "add more real-client traces"
    }
  ]
}"#,
    )
    .unwrap();
    fs::write(
        root.join("eval")
            .join("fixtures")
            .join("runtime")
            .join("edge-claude-api-public-http-tools-only.json"),
        r#"{
  "id": "edge-claude-api-public-http-tools-only",
  "suite": "compat",
  "category": "edge",
  "proofLayer": "planner",
  "heldOut": false,
  "title": "Claude API connector needs public HTTP tools-only routing",
  "objective": "Keep tools-only and public HTTP constraints explicit",
  "traffic": {
    "clientArchetype": "claude-api-connector",
    "serverPolicies": ["shared-global"],
    "signals": ["clientInfo", "_meta"]
  },
  "checks": ["must-warn-public-http-only", "must-treat-surface-as-tools-only"],
  "requires": ["planner-client-surface-catalog", "planner-surface-constraint-warnings"]
}"#,
    )
    .unwrap();
    fs::write(
        root.join("eval")
            .join("fixtures")
            .join("runtime")
            .join("edge-copilot-cloud-tools-only-no-oauth.json"),
        r#"{
  "id": "edge-copilot-cloud-tools-only-no-oauth",
  "suite": "compat",
  "category": "edge",
  "proofLayer": "planner",
  "heldOut": false,
  "title": "Copilot cloud agent keeps tools-only and no-oauth explicit",
  "objective": "Keep cloud-agent limits visible in lab coverage",
  "traffic": {
    "clientArchetype": "github-copilot-cloud-agent",
    "serverPolicies": ["shared-global"],
    "signals": ["clientInfo", "transportSessionId"]
  },
  "checks": ["must-treat-surface-as-tools-only"],
  "requires": ["planner-client-surface-catalog"]
}"#,
    )
    .unwrap();

    let output = run(&[
        "lab",
        "coverage",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""surfaceClasses": {"#));
    assert!(text.contains(r#""cloud": 2"#));
    assert!(text.contains(r#""documentedConstraints": {"#));
    assert!(text.contains(r#""tools-only": 2"#));
    assert!(text.contains(r#""public-http-only": 1"#));
    assert!(text.contains(r#""unknownClientArchetypes": []"#));
}

#[test]
fn lab_report_json_turns_runtime_fixtures_into_gap_backlog() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::create_dir_all(root.join("eval").join("fixtures").join("runtime")).unwrap();
    fs::write(
        root.join("eval").join("runtime-capabilities.json"),
        r#"{
  "version": "0.3.5",
  "features": [
    {
      "id": "planner-context-resolution",
      "area": "planner",
      "title": "Context resolution",
      "status": "implemented",
      "priority": "p0",
      "summary": "done",
      "evidence": ["src/client.rs"],
      "nextStep": "keep reused by the hub"
    },
    {
      "id": "runtime-hub-daemon",
      "area": "runtime",
      "title": "Live hub daemon",
      "status": "planned",
      "priority": "p0",
      "summary": "missing",
      "evidence": ["README.md"],
      "nextStep": "implement hub up/status/stop"
    }
  ]
}"#,
    )
    .unwrap();
    fs::write(
        root.join("eval")
            .join("fixtures")
            .join("runtime")
            .join("typical-project-routing.json"),
        r#"{
  "id": "typical-project-routing",
  "suite": "routing",
  "category": "typical",
  "proofLayer": "planner",
  "heldOut": false,
  "title": "Path based routing",
  "objective": "Use path/roots to resolve a project",
  "traffic": {
    "clientArchetype": "generic-stdio",
    "serverPolicies": ["project-local"],
    "signals": ["path", "roots"]
  },
  "checks": ["resolved-project"],
  "requires": ["planner-context-resolution"]
}"#,
    )
    .unwrap();
    fs::write(
        root.join("eval")
            .join("fixtures")
            .join("runtime")
            .join("adversarial-double-owner.json"),
        r#"{
  "id": "adversarial-double-owner",
  "suite": "leases",
  "category": "adversarial",
  "proofLayer": "runtime",
  "heldOut": false,
  "title": "Exclusive owner conflict",
  "objective": "Single-session servers must reject a second owner",
  "traffic": {
    "clientArchetype": "generic-http",
    "serverPolicies": ["single-session"],
    "signals": ["session-id"]
  },
  "checks": ["single-owner"],
  "requires": ["runtime-hub-daemon"]
}"#,
    )
    .unwrap();

    let output = run(&["lab", "report", "--json", "--root", root.to_str().unwrap()]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""scenarioCount": 2"#));
    assert!(text.contains(r#""covered-now": 1"#));
    assert!(text.contains(r#""blocked": 1"#));
    assert!(text.contains(r#""capabilityId": "runtime-hub-daemon""#));
    assert!(text.contains(r#""nextSteps": ["#));
}

#[test]
fn lab_show_json_returns_specific_scenario_and_outstanding_requirements() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::create_dir_all(root.join("eval").join("fixtures").join("runtime")).unwrap();
    fs::write(
        root.join("eval").join("runtime-capabilities.json"),
        r#"{
  "version": "0.3.5",
  "features": [
    {
      "id": "runtime-stdio-shim",
      "area": "runtime",
      "title": "stdio shim",
      "status": "planned",
      "priority": "p0",
      "summary": "missing",
      "evidence": ["TODO.md"],
      "nextStep": "add mcpace stdio-shim"
    }
  ]
}"#,
    )
    .unwrap();
    fs::write(
        root.join("eval")
            .join("fixtures")
            .join("runtime")
            .join("edge-no-roots-meta-only.json"),
        r#"{
  "id": "edge-no-roots-meta-only",
  "suite": "ingress",
  "category": "edge",
  "proofLayer": "runtime",
  "heldOut": false,
  "title": "Metadata only fallback",
  "objective": "Accept metadata-only hints when roots are absent",
  "traffic": {
    "clientArchetype": "generic-stdio",
    "serverPolicies": ["project-local"],
    "signals": ["cwd", "_meta"]
  },
  "checks": ["sticky-lease"],
  "requires": ["runtime-stdio-shim"]
}"#,
    )
    .unwrap();

    let output = run(&[
        "lab",
        "show",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--id",
        "edge-no-roots-meta-only",
    ]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""readiness": "blocked""#));
    assert!(text.contains(r#""id": "edge-no-roots-meta-only""#));
    assert!(text.contains(r#""outstanding": ["#));
    assert!(text.contains(r#""runtime-stdio-shim""#));
}
