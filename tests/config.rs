use rulist::config::Config;

#[test]
fn fresh_config_has_all_sections_and_legacy_config_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let (config, config_path) = Config::load_or_create(temp.path()).unwrap();
    let content = std::fs::read_to_string(config_path).unwrap();
    assert!(content.contains("\"server\""));
    assert!(content.contains("\"security\""));
    assert!(content.contains("\"ui\""));
    assert!(content.contains("\"database\""));
    assert!(!config.security.jwt_secret.is_empty());
    assert_ne!(config.security.jwt_secret, config.security.signing_secret);

    let mut missing_signing_secret = serde_json::to_value(&config).unwrap();
    missing_signing_secret["security"]
        .as_object_mut()
        .unwrap()
        .remove("signing_secret");
    assert!(serde_json::from_value::<Config>(missing_signing_secret).is_err());

    std::fs::write(
        temp.path().join("config.json"),
        r#"{"jwt_secret":"legacy"}"#,
    )
    .unwrap();
    assert!(Config::load_or_create(temp.path()).is_err());
}
