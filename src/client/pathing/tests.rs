use super::path_is_within;

#[test]
fn path_containment_uses_lexical_segments_not_raw_prefixes() {
    assert!(path_is_within("/work/project/src/lib.rs", "/work/project"));
    assert!(!path_is_within(
        "/work/project/../secret/file.txt",
        "/work/project"
    ));
    assert!(!path_is_within(
        "/work/project-other/file.txt",
        "/work/project"
    ));
}

#[test]
fn path_containment_handles_windows_drive_roots_case_insensitively() {
    assert!(path_is_within("c:/Users/alice/project", "C:/"));
    assert!(path_is_within("C:/Work/Project", "c:/work"));
    assert!(!path_is_within("C:/work/../Windows", "C:/work"));
    assert!(!path_is_within("D:/Users/alice", "C:/"));
}
