use super::{score_search_server_fields, search_terms, DEFAULT_UPSTREAM_SEARCH_MIN_SERVER_SCORE};

#[test]
fn browser_action_terms_rank_browser_servers() {
    let terms = search_terms("click screenshot localhost");
    let browser_score = score_search_server_fields("browser", "stdio chrome cdp", &terms);
    let playwright_score =
        score_search_server_fields("playwright", "stdio browser automation", &terms);
    let filesystem_score = score_search_server_fields("filesystem", "stdio file read", &terms);

    assert!(browser_score > filesystem_score);
    assert!(playwright_score > filesystem_score);
    assert!(browser_score >= 90);
}

#[test]
fn unrelated_terms_do_not_force_browser_candidates() {
    let terms = search_terms("database table query");
    let browser_score = score_search_server_fields("browser", "stdio chrome cdp", &terms);
    let sqlite_score = score_search_server_fields("sqlite", "stdio database table query", &terms);

    assert_eq!(browser_score, 0);
    assert!(sqlite_score > browser_score);
}

#[test]
fn weak_metadata_matches_stay_below_candidate_threshold() {
    let terms = search_terms("totally unknown quantum banana spaceship");
    let weak_score = score_search_server_fields("everything", "unknown stdio server", &terms);
    let browser_score = score_search_server_fields("browser", "stdio chrome cdp", &terms);

    assert!(weak_score > 0);
    assert!(weak_score < DEFAULT_UPSTREAM_SEARCH_MIN_SERVER_SCORE);
    assert_eq!(browser_score, 0);
}
