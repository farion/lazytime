use lazytime::config::Config;
use tempfile::tempdir;

#[test]
fn config_is_auto_created_when_missing() {
    let dir = tempdir().expect("tempdir");
    let cfg_path = dir.path().join("nested/lazytime/config.json");
    let cfg_path_str = cfg_path.to_string_lossy().to_string();

    let cfg = Config::from_path(Some(&cfg_path_str)).expect("auto-create config");

    assert!(cfg_path.exists());
    assert_eq!(cfg.default_project, "Default");
    assert!(!cfg.db_file.is_empty());
}
