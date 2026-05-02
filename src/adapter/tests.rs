use super::*;

#[test]
fn safe_projected_names_are_bounded_and_unique() {
    let mut used = BTreeSet::new();
    let first = unique_projected_name("u", "Example Server", "read/file", &mut used);
    let second = unique_projected_name("u", "Example Server", "read file", &mut used);
    assert!(first.len() <= PROJECTED_NAME_MAX);
    assert!(second.len() <= PROJECTED_NAME_MAX);
    assert_ne!(first, second);
}

#[test]
fn resource_uri_round_trips() {
    let uri = encode_resource_uri("filesystem", "file:///tmp/hello world.txt");
    let (server, upstream_uri) = decode_resource_uri(&uri).unwrap();
    assert_eq!(server, "filesystem");
    assert_eq!(upstream_uri, "file:///tmp/hello world.txt");
}
