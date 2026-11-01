use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::cache::ShardCache;
use crate::error::{Error, Result};
use crate::host::{host_of, HostSet};
use crate::model::{
    CatalogItem, EngineConfig, HostsFile, Manifest, SearchHit, SearchResults, ShardFile,
};
use crate::normalize::{normalize_tag, tag_prefix, tokenize};
use crate::rank::score;

pub const FEATURED_QUERIES: &[&str] = &[
    "wave",
    "cat",
    "dance",
    "thumbs up",
    "party",
    "heart",
    "laugh",
    "hello",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchPlan {
    pub tokens: Vec<String>,
    pub prefixes: Vec<String>,
    pub paths: Vec<String>,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStats {
    pub manifest_loaded: bool,
    pub total_shards: usize,
    pub cached_shards: usize,
    pub cached_items: usize,
    pub hosts_configured: bool,
}

#[derive(Debug)]
pub(crate) struct ItemDoc {
    pub item: CatalogItem,
    tags: Vec<String>,
    title: String,
    url_host: Option<String>,
    preview_host: Option<String>,
}

impl ItemDoc {
    pub(crate) fn new(mut item: CatalogItem, require_https: bool) -> Option<Self> {
        if item.id.is_empty() || item.url.is_empty() {
            return None;
        }
        if item.preview_url.is_empty() {
            item.preview_url = item.url.clone();
        }
        if item.mime.is_empty() {
            item.mime = "image/gif".to_string();
        }
        if !item.score.is_finite() {
            item.score = 0.5;
        }
        item.score = item.score.clamp(0.0, 1.0);

        let url_host = host_of(&item.url, require_https);
        let preview_host = host_of(&item.preview_url, require_https);
        let tags = item
            .tags
            .iter()
            .map(|tag| normalize_tag(tag))
            .filter(|tag| !tag.is_empty())
            .collect();
        Some(Self {
            title: normalize_tag(&item.title),
            tags,
            url_host,
            preview_host,
            item,
        })
    }

    fn is_servable(&self, hosts: &HostSet) -> bool {
        match (&self.url_host, &self.preview_host) {
            (Some(url_host), Some(preview_host)) => {
                hosts.allows_host(url_host) && hosts.allows_host(preview_host)
            }
            _ => false,
        }
    }

    fn rank(&self, tokens: &[String]) -> Option<f64> {
        score(&self.tags, &self.title, self.item.score, tokens)
    }
}

#[derive(Debug)]
pub struct Engine {
    config: EngineConfig,
    manifest: Option<Manifest>,
    shard_index: HashMap<String, Vec<usize>>,
    hosts: Option<HostSet>,
    cache: ShardCache,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Self::with_config(EngineConfig::default()).expect("default engine config is valid")
    }

    pub fn with_config(config: EngineConfig) -> Result<Self> {
        if config.max_shard_fetches == 0 {
            return Err(Error::Config("max_shard_fetches must be at least 1".into()));
        }
        if config.max_cached_shards == 0 {
            return Err(Error::Config("max_cached_shards must be at least 1".into()));
        }
        if config.max_cached_items == 0 {
            return Err(Error::Config("max_cached_items must be at least 1".into()));
        }
        if !config.min_rank.is_finite() {
            return Err(Error::Config("min_rank must be finite".into()));
        }
        Ok(Self {
            cache: ShardCache::new(config.max_cached_shards, config.max_cached_items),
            config,
            manifest: None,
            shard_index: HashMap::new(),
            hosts: None,
        })
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    pub fn reset(&mut self) {
        self.manifest = None;
        self.shard_index.clear();
        self.hosts = None;
        self.cache.clear();
    }

    pub fn clear_shards(&mut self) {
        self.cache.clear();
    }

    pub fn manifest(&self) -> Option<&Manifest> {
        self.manifest.as_ref()
    }

    pub fn set_manifest_json(&mut self, json: &str) -> Result<()> {
        let manifest: Manifest = serde_json::from_str(json)?;
        self.set_manifest(manifest)
    }

    pub fn set_manifest(&mut self, manifest: Manifest) -> Result<()> {
        if !manifest.schema_version.is_empty() && !manifest.schema_version.starts_with("1.") {
            return Err(Error::UnsupportedSchema(manifest.schema_version.clone()));
        }
        let mut shard_index: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, shard) in manifest.shards.iter().enumerate() {
            if shard.path.is_empty() {
                continue;
            }
            shard_index
                .entry(tag_prefix(&shard.prefix))
                .or_default()
                .push(index);
        }
        self.shard_index = shard_index;
        self.manifest = Some(manifest);
        self.cache.clear();
        Ok(())
    }

    pub fn set_hosts_json(&mut self, json: &str) -> Result<()> {
        let file: HostsFile = serde_json::from_str(json)?;
        self.hosts = Some(file.hosts);
        Ok(())
    }

    pub fn set_hosts(&mut self, hosts: HostSet) {
        self.hosts = Some(hosts);
    }

    pub fn use_default_hosts(&mut self) {
        self.hosts = None;
    }

    pub fn allowed_hosts(&self) -> &HostSet {
        match &self.hosts {
            Some(hosts) => hosts,
            None => default_hosts(),
        }
    }

    pub fn insert_shard_json(&mut self, path: &str, json: &str) -> Result<usize> {
        let shard: ShardFile = serde_json::from_str(json)?;
        Ok(self.insert_shard(path, shard))
    }

    pub fn insert_shard(&mut self, path: &str, shard: ShardFile) -> usize {
        let docs: Vec<ItemDoc> = shard
            .items
            .into_iter()
            .filter_map(|item| ItemDoc::new(item, self.config.require_https))
            .collect();
        let count = docs.len();
        self.cache.insert(path.to_string(), docs);
        count
    }

    pub fn has_shard(&self, path: &str) -> bool {
        self.cache.contains(path)
    }

    pub fn plan(&self, query: &str) -> Result<SearchPlan> {
        let tokens = tokenize(query);
        let manifest = self.manifest.as_ref().ok_or(Error::ManifestNotLoaded)?;

        let mut prefixes: Vec<String> = Vec::new();
        let mut seen_prefixes: HashSet<String> = HashSet::new();
        for token in &tokens {
            let prefix = tag_prefix(token);
            if seen_prefixes.insert(prefix.clone()) {
                prefixes.push(prefix);
            }
        }

        let mut paths: Vec<String> = Vec::new();
        let mut seen_paths: HashSet<String> = HashSet::new();
        for prefix in &prefixes {
            let Some(indices) = self.shard_index.get(prefix) else {
                continue;
            };
            for index in indices {
                let path = &manifest.shards[*index].path;
                if seen_paths.insert(path.clone()) {
                    paths.push(path.clone());
                }
            }
        }
        paths.truncate(self.config.max_shard_fetches);

        let missing = paths
            .iter()
            .filter(|path| !self.cache.contains(path))
            .cloned()
            .collect();
        Ok(SearchPlan {
            tokens,
            prefixes,
            paths,
            missing,
        })
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<SearchResults> {
        let plan = self.plan(query)?;
        Ok(self.search_planned(&plan, limit))
    }

    pub fn search_planned(&self, plan: &SearchPlan, limit: usize) -> SearchResults {
        if limit == 0 || plan.tokens.is_empty() {
            return SearchResults {
                hits: Vec::new(),
                missing_shards: self.missing_shards(plan),
            };
        }
        let hosts = self.allowed_hosts();
        let mut seen: HashSet<&str> = HashSet::new();
        let mut hits: Vec<SearchHit> = Vec::new();

        for path in &plan.paths {
            let Some(docs) = self.cache.get(path) else {
                continue;
            };
            for doc in docs {
                if !doc.is_servable(hosts) || !seen.insert(doc.item.id.as_str()) {
                    continue;
                }
                if let Some(rank) = doc.rank(&plan.tokens) {
                    if rank > self.config.min_rank {
                        hits.push(SearchHit {
                            item: doc.item.clone(),
                            rank,
                        });
                    }
                }
            }
        }

        hits.sort_by(|a, b| {
            b.rank
                .total_cmp(&a.rank)
                .then_with(|| a.item.title.cmp(&b.item.title))
                .then_with(|| a.item.id.cmp(&b.item.id))
        });
        hits.truncate(limit);
        SearchResults {
            hits,
            missing_shards: self.missing_shards(plan),
        }
    }

    fn missing_shards(&self, plan: &SearchPlan) -> Vec<String> {
        plan.paths
            .iter()
            .filter(|path| !self.cache.contains(path))
            .cloned()
            .collect()
    }

    pub fn stats(&self) -> EngineStats {
        EngineStats {
            manifest_loaded: self.manifest.is_some(),
            total_shards: self.manifest.as_ref().map_or(0, |m| m.shards.len()),
            cached_shards: self.cache.len(),
            cached_items: self.cache.item_count(),
            hosts_configured: self.hosts.is_some(),
        }
    }
}

fn default_hosts() -> &'static HostSet {
    static DEFAULT: OnceLock<HostSet> = OnceLock::new();
    DEFAULT.get_or_init(|| HostSet::from_hosts(["upload.wikimedia.org", "thumb.wikimedia.org"]))
}
