#[test]
fn the_service_needs_an_index_directory() {
    let run = |dir: Option<&str>| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_opengeocoding"));
        command.env_remove("INDEX_DIR").env("SENTRY_DSN", "");
        if let Some(dir) = dir {
            command.env("INDEX_DIR", dir);
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        String::from_utf8_lossy(&output.stderr).into_owned()
    };
    assert!(run(None).contains("INDEX_DIR is required"));
    let empty = std::env::temp_dir().join(format!("no-index-{}", std::process::id()));
    std::fs::create_dir_all(&empty).unwrap();
    let error = run(empty.to_str());
    std::fs::remove_dir(&empty).unwrap();
    assert!(error.contains("holds no build_index output"), "{error}");
}
