use std::path::{Path, PathBuf};

use clampey::{Engine, EngineConfig, Manifest, SearchResults};

fn catalog_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

fn read(name: &str, relative: &str) -> String {
    std::fs::read_to_string(catalog_dir(name).join(relative)).unwrap()
}

fn manifest(name: &str) -> Manifest {
    serde_json::from_str(&read(name, "manifest.json")).unwrap()
}

fn engine_for(name: &str) -> Engine {
    let manifest = manifest(name);
    let mut engine = Engine::new();
    engine.set_manifest(manifest.clone()).unwrap();
    engine.set_hosts_json(&read(name, "hosts.json")).unwrap();
    for shard in &manifest.shards {
        engine
            .insert_shard_json(&shard.path, &read(name, &shard.path))
            .unwrap();
    }
    engine
}

fn ids(results: &SearchResults) -> Vec<&str> {
    results
        .hits
        .iter()
        .map(|hit| hit.item.id.as_str())
        .collect()
}

#[test]
fn ranks_exact_tag_matches_ahead_of_lower_scores() {
    let results = engine_for("catalog").search("cat", 10).unwrap();
    assert_eq!(ids(&results), ["cat-curated", "cat-both", "cat-wave"]);
    assert!(results.missing_shards.is_empty());
}

#[test]
fn excludes_items_whose_tags_do_not_match() {
    let results = engine_for("catalog").search("cat", 10).unwrap();
    assert!(!ids(&results).contains(&"car-curated"));
}

#[test]
fn filters_media_outside_the_host_allowlist_or_https() {
    let results = engine_for("catalog-edge").search("cat", 10).unwrap();
    assert_eq!(ids(&results), ["cat-ok"]);
}

#[test]
fn dedupes_items_posted_to_several_shards() {
    let results = engine_for("catalog").search("dance", 10).unwrap();
    assert_eq!(results.hits.first().unwrap().item.id, "cat-both");
    assert_eq!(
        results
            .hits
            .iter()
            .filter(|hit| hit.item.id == "cat-both")
            .count(),
        1
    );
}

#[test]
fn plans_only_the_shards_for_the_query_prefixes() {
    let engine = engine_for("catalog");
    let plan = engine.plan("thumbs up").unwrap();
    assert_eq!(plan.tokens, ["thumbs", "up"]);
    assert_eq!(plan.prefixes, ["th", "up"]);
    assert_eq!(plan.paths, ["shards/th.json"]);
    assert!(plan.missing.is_empty());
}

#[test]
fn reports_missing_shards_before_they_are_ingested() {
    let mut engine = Engine::new();
    engine.set_manifest(manifest("catalog")).unwrap();

    let plan = engine.plan("cat").unwrap();
    assert_eq!(plan.paths, ["shards/ca.json"]);
    assert_eq!(plan.missing, ["shards/ca.json"]);

    let results = engine.search("cat", 10).unwrap();
    assert!(results.hits.is_empty());
    assert_eq!(results.missing_shards, ["shards/ca.json"]);
}

#[test]
fn evicts_least_recently_used_shards() {
    let config = EngineConfig {
        max_cached_shards: 1,
        ..EngineConfig::default()
    };
    let mut engine = Engine::with_config(config).unwrap();
    engine.set_manifest(manifest("catalog")).unwrap();
    engine.use_default_hosts();
    engine
        .insert_shard_json("shards/ca.json", &read("catalog", "shards/ca.json"))
        .unwrap();
    engine
        .insert_shard_json("shards/da.json", &read("catalog", "shards/da.json"))
        .unwrap();

    assert_eq!(engine.plan("cat").unwrap().missing, ["shards/ca.json"]);
    assert!(engine.plan("dance").unwrap().missing.is_empty());
}

#[test]
fn empty_queries_produce_no_hits() {
    let results = engine_for("catalog").search("   ", 10).unwrap();
    assert!(results.hits.is_empty());
    assert!(results.missing_shards.is_empty());
}

#[test]
fn stats_track_the_cache() {
    let stats = engine_for("catalog").stats();
    assert!(stats.manifest_loaded);
    assert_eq!(stats.total_shards, 7);
    assert_eq!(stats.cached_shards, 7);
    assert!(stats.hosts_configured);
}

#[test]
fn rejects_unknown_schema_versions() {
    let mut engine = Engine::new();
    let mut manifest = manifest("catalog");
    manifest.schema_version = "2.0.0".to_string();
    assert!(engine.set_manifest(manifest).is_err());
}
