use serde::{Deserialize, Serialize};

use crate::host::HostSet;

pub const SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub schema_version: String,
    #[serde(default)]
    pub catalog_version: String,
    #[serde(default)]
    pub generated_at: Option<String>,
    #[serde(default)]
    pub item_count: u64,
    #[serde(default)]
    pub shard_count: u64,
    #[serde(default)]
    pub shard_by: Option<String>,
    #[serde(default)]
    pub shards: Vec<ShardRef>,
    #[serde(default)]
    pub tags_index_path: Option<String>,
    #[serde(default)]
    pub hosts_path: Option<String>,
    #[serde(default)]
    pub base_url_hint: Option<String>,
    #[serde(default)]
    pub license_note: Option<String>,
    #[serde(default)]
    pub min_client_features: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShardRef {
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub item_count: u64,
    #[serde(default)]
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShardFile {
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub items: Vec<CatalogItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostsFile {
    #[serde(default)]
    pub hosts: HostSet,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TagsFile {
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub preview_url: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub size: u64,
    #[serde(default = "default_mime")]
    pub mime: String,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub license_url: String,
    #[serde(default)]
    pub attribution: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub author_url: Option<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub source_page: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default = "default_score")]
    pub score: f64,
}

fn default_mime() -> String {
    "image/gif".to_string()
}

fn default_score() -> f64 {
    0.5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    #[serde(flatten)]
    pub item: CatalogItem,
    pub rank: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    #[serde(default)]
    pub missing_shards: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    pub max_shard_fetches: usize,
    pub max_cached_shards: usize,
    pub max_cached_items: usize,
    pub min_rank: f64,
    pub require_https: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_shard_fetches: 8,
            max_cached_shards: 16,
            max_cached_items: 50_000,
            min_rank: 0.5,
            require_https: true,
        }
    }
}
