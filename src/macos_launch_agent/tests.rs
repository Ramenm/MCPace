use super::{launch_agent_plist, service_target};

#[test]
fn launch_agent_paths_and_targets_use_the_exact_label() {
    let label = "MCPace Agent";
    let target = service_target(label);
    assert!(target.starts_with("gui/"));
    assert!(target.ends_with("/MCPace Agent"));
    let plist = launch_agent_plist(label).unwrap();
    assert_eq!(plist.file_name().unwrap(), "MCPace Agent.plist");
}
