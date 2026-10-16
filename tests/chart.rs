//! The chart names indexes by the builder hash it pins, and the build Job refuses an image whose
//! builder differs, so a builder change must update `manifests/values.yaml` in the same commit.
#[test]
fn chart_pins_this_builder() {
    let values = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/manifests/values.yaml"
    ))
    .unwrap();
    let pinned = values
        .lines()
        .find_map(|l| l.trim().strip_prefix("builderHash: "))
        .expect("index.builderHash in manifests/values.yaml")
        .trim_matches('"');
    assert_eq!(
        pinned,
        env!("BUILDER_HASH"),
        "the index builder changed: set index.builderHash in manifests/values.yaml to {} \
         (`build_index hash`), which makes the chart build a new index",
        env!("BUILDER_HASH")
    );
}

/// libpostal has no runtime data-dir setting (`rustpostal` calls plain `libpostal_setup`), so
/// every `--models` the chart passes must be the `--datadir` the Dockerfile compiles in, plus
/// `/libpostal`; the build Job's Verify step loads the models from there.
#[test]
fn chart_models_are_where_libpostal_reads() {
    let read =
        |p: &str| std::fs::read_to_string(format!("{}/{p}", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let dockerfile = read("Dockerfile");
    let datadir = dockerfile
        .split_whitespace()
        .find_map(|w| w.strip_prefix("--datadir="))
        .expect("libpostal --datadir in the Dockerfile");
    let expected = format!("- {datadir}/libpostal");
    let chart = read("manifests/templates/_workloads.tpl");
    let lines: Vec<&str> = chart.lines().map(str::trim).collect();
    let models: Vec<&str> = lines
        .windows(2)
        .filter(|w| w[0] == "- --models")
        .map(|w| w[1])
        .collect();
    assert!(!models.is_empty(), "no --models in the chart");
    for m in models {
        assert_eq!(
            m, expected,
            "--models must be libpostal's compiled-in data dir"
        );
    }
}
