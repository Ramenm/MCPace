use super::{capture, match_process, ProcessMatch};

#[test]
fn current_process_identity_is_stable_and_matches_executable() {
    let pid = std::process::id();
    let first = capture(pid).unwrap().expect("current process identity");
    let second = capture(pid).unwrap().expect("current process identity");
    assert_eq!(first.start_token, second.start_token);
    assert_eq!(
        match_process(pid, Some(&first.start_token), first.executable.as_deref()).unwrap(),
        ProcessMatch::Match
    );
    assert_eq!(
        match_process(pid, Some("wrong-process-identity"), None).unwrap(),
        ProcessMatch::Mismatch
    );
    assert_eq!(
        match_process(pid, None, None).unwrap(),
        ProcessMatch::Mismatch
    );
}
