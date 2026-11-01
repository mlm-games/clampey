use clampey::{normalize_tag, tag_prefix, tokenize};
use serde::Deserialize;

#[derive(Deserialize)]
struct Vector {
    input: String,
    normalized: String,
    prefix: String,
}

fn vectors() -> Vec<Vector> {
    serde_json::from_str(include_str!("../fixtures/prefix_vectors.json")).unwrap()
}

#[test]
fn golden_prefix_vectors() {
    let vectors = vectors();
    assert!(!vectors.is_empty());
    for vector in vectors {
        assert_eq!(
            normalize_tag(&vector.input),
            vector.normalized,
            "normalizing {:?}",
            vector.input
        );
        assert_eq!(
            tag_prefix(&vector.input),
            vector.prefix,
            "prefixing {:?}",
            vector.input
        );
    }
}

#[test]
fn tokens_are_normalized_and_deduped() {
    assert_eq!(tokenize("  Thumbs   UP thumbs "), ["thumbs", "up"]);
}
