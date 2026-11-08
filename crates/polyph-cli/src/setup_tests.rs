use super::*;

#[test]
fn git_name_is_used_when_nothing_is_saved() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("gitconfig");
    std::fs::write(&cfg, "[user]\n\tname = Ann\n").unwrap();
    let found = git_user_name(Some(&cfg));
    assert_eq!(found.as_deref(), Some("Ann"));
    assert_eq!(name_source(found.clone(), false), NameSource::Git("Ann".into()));
    assert_eq!(name_source(found, true), NameSource::Git("Ann".into()));
}

#[test]
fn no_git_name_asks_on_a_terminal_and_fails_otherwise() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("empty");
    std::fs::write(&cfg, "").unwrap();
    let found = git_user_name(Some(&cfg));
    assert_eq!(found, None);
    assert_eq!(name_source(found.clone(), true), NameSource::Ask);
    assert_eq!(name_source(found, false), NameSource::Missing);
}
