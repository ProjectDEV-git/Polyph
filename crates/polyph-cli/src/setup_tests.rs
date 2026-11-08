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

#[test]
fn only_the_default_name_can_replace_another_homes_entry() {
    let other = json!({ "command": "/dev/polyph", "args": ["mcp"], "env": { "POLYPH_HOME": "/tmp/other" } });
    let plain = json!({ "command": "/usr/bin/polyph", "args": ["mcp"] });
    // Default home, entry for another home: would replace it.
    assert!(replaces_other_home("polyph", Some(&other), None));
    // Default home, entry without a home: it is the default home's own.
    assert!(!replaces_other_home("polyph", Some(&plain), None));
    assert!(!replaces_other_home("polyph", None, None));
    // The same home again is not a replacement.
    assert!(!replaces_other_home("polyph", Some(&other), Some(Path::new("/tmp/other"))));
    // A home's own name is made from its path, so it never hits another home's.
    assert!(!replaces_other_home("polyph-1a2b3c4d", Some(&other), Some(Path::new("/tmp/x"))));
}
