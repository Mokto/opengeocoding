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
