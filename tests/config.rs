use rulist::config::Config;

#[test]
fn fresh_config_has_final_sections_and_legacy_config_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let (config, config_path) = Config::load_or_create(temp.path()).unwrap();
    let content = std::fs::read_to_string(config_path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(
        json.as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        [
            "database",
            "jwt_secret",
            "scheme",
            "site",
            "token_expires_in"
        ]
        .into_iter()
        .collect()
    );
    assert!(content.contains("\"jwt_secret\""));
    assert!(content.contains("\"scheme\""));
    assert!(content.contains("\"site\""));
    assert!(content.contains("\"database\""));
    assert!(!config.jwt_secret.is_empty());
    assert_eq!(config.database.db_file, "data.db");
    assert!(config.site.robots_txt.contains("Allow: /"));

    std::fs::write(
        temp.path().join("config.json"),
        r#"{"security":{"jwt_secret":"legacy"}}"#,
    )
    .unwrap();
    assert!(Config::load_or_create(temp.path()).is_err());
}
