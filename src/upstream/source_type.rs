pub(super) fn infer_source_type(raw_source_type: &str, command: &str, url: &str) -> String {
    crate::source_type::infer_runtime_source_type(raw_source_type, command, url)
}
