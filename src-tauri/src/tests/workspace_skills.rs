use super::*;

#[test]
fn skill_source_views_serialize_with_camel_case_fields() -> Result<()> {
    let local = crate::workspace::types::SkillSourceConfigView::Local {
        id: "local-source".to_string(),
        root_path: "/tmp/skills".to_string(),
        include_name_patterns: vec!["foo*".to_string()],
        include_path_patterns: vec![],
        label: "Local".to_string(),
    };

    let local_value = serde_json::to_value(local)?;

    assert_eq!(
        local_value.get("type").and_then(Value::as_str),
        Some("local")
    );
    assert_eq!(
        local_value.get("rootPath").and_then(Value::as_str),
        Some("/tmp/skills")
    );
    assert_eq!(
        local_value
            .get("includeNamePatterns")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(Value::as_str),
        Some("foo*")
    );
    assert!(local_value.get("root_path").is_none());
    assert!(local_value.get("exclude_patterns").is_none());

    Ok(())
}

#[test]
fn git_skill_source_view_serializes_last_fetched_at() -> Result<()> {
    let git = crate::workspace::types::SkillSourceConfigView::Git {
        id: "git-source".to_string(),
        repo: "https://github.com/acme/skills.git".to_string(),
        r#ref: Some("main".to_string()),
        last_fetched_at: Some(1_744_366_400_000),
        include_name_patterns: Vec::new(),
        include_path_patterns: Vec::new(),
        label: "Git".to_string(),
    };

    let git_value = serde_json::to_value(git)?;

    assert_eq!(
        git_value.get("lastFetchedAt").and_then(Value::as_i64),
        Some(1_744_366_400_000)
    );
    assert!(git_value.get("last_fetched_at").is_none());

    Ok(())
}

#[test]
#[ignore = "requires network access"]
fn discover_git_skills_smoke() -> Result<()> {
    let state = crate::state::skill_discovery::SkillDiscoveryState::default();
    let store = crate::workspace::WorkspaceConfigStore::app();
    let result = crate::workspace::discover_git_skills_inner(
        &store,
        &state,
        "https://github.com/anthropics/skills.git",
        Some("main"),
    )?;

    assert!(!result.skills.is_empty());
    assert!(
        result
            .skills
            .iter()
            .any(|skill| skill.name == "frontend-design")
    );
    assert!(
        result
            .skills
            .iter()
            .all(|skill| !skill.relative_path.is_empty())
    );
    Ok(())
}
