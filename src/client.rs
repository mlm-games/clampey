use std::collections::{BTreeSet, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use futures_util::future::join_all;
use url::Url;

use crate::engine::{Engine, FEATURED_QUERIES};
use crate::error::{Error, Result};
use crate::model::{EngineConfig, HostsFile, Manifest, SearchResults, TagsFile};
use crate::normalize::normalize_tag;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchError {
    pub status: Option<u16>,
    pub message: String,
}

impl FetchError {
    pub fn status(code: u16) -> Self {
        Self {
            status: Some(code),
            message: format!("HTTP {code}"),
        }
    }

    pub fn transport(message: impl Into<String>) -> Self {
        Self {
            status: None,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for FetchError {}

#[async_trait]
pub trait Fetcher: Send + Sync {
    async fn get(&self, url: &str) -> std::result::Result<String, FetchError>;
}

#[async_trait]
impl<F, Fut> Fetcher for F
where
    F: Fn(String) -> Fut + Send + Sync,
    Fut: Future<Output = std::result::Result<String, FetchError>> + Send,
{
    async fn get(&self, url: &str) -> std::result::Result<String, FetchError> {
        (self)(url.to_owned()).await
    }
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub engine: EngineConfig,
    pub max_concurrent_fetches: usize,
    pub allow_insecure_base: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            engine: EngineConfig::default(),
            max_concurrent_fetches: 4,
            allow_insecure_base: false,
        }
    }
}

pub struct CatalogClient<F: Fetcher> {
    fetcher: F,
    config: ClientConfig,
    state: Mutex<State>,
}

struct State {
    base: String,
    epoch: u64,
    engine: Engine,
    manifest: Option<Manifest>,
    ready: bool,
    tags: Option<Arc<Vec<String>>>,
}

impl<F: Fetcher> CatalogClient<F> {
    pub fn new(fetcher: F, base_url: &str) -> Result<Self> {
        Self::with_config(fetcher, base_url, ClientConfig::default())
    }

    pub fn with_config(fetcher: F, base_url: &str, config: ClientConfig) -> Result<Self> {
        let base = normalize_base(base_url, config.allow_insecure_base)?;
        let engine = Engine::with_config(config.engine.clone())?;
        Ok(Self {
            fetcher,
            config,
            state: Mutex::new(State {
                base,
                epoch: 0,
                engine,
                manifest: None,
                ready: false,
                tags: None,
            }),
        })
    }

    pub fn base_url(&self) -> String {
        self.state().base.clone()
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    pub fn set_base_url(&self, base_url: &str) -> Result<()> {
        let base = normalize_base(base_url, self.config.allow_insecure_base)?;
        let mut state = self.state();
        state.base = base;
        state.epoch += 1;
        state.engine.reset();
        state.manifest = None;
        state.ready = false;
        state.tags = None;
        Ok(())
    }

    pub fn refresh(&self) {
        let mut state = self.state();
        state.epoch += 1;
        state.engine.reset();
        state.manifest = None;
        state.ready = false;
        state.tags = None;
    }

    pub fn stats(&self) -> crate::EngineStats {
        self.state().engine.stats()
    }

    pub async fn manifest(&self) -> Result<Manifest> {
        self.ensure_ready().await?;
        self.state()
            .manifest
            .clone()
            .ok_or(Error::ManifestNotLoaded)
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<SearchResults> {
        for _ in 0..2 {
            self.ensure_ready().await?;
            let (epoch, plan) = {
                let state = self.state();
                (state.epoch, state.engine.plan(query)?)
            };
            self.load_shards(&plan.missing, epoch).await?;
            let results = {
                let state = self.state();
                if state.epoch != epoch {
                    None
                } else {
                    Some(state.engine.search(query, limit)?)
                }
            };
            if let Some(results) = results {
                return Ok(results);
            }
        }
        Err(Error::Config("catalog changed while searching".into()))
    }

    pub async fn featured(&self, limit: usize) -> Result<SearchResults> {
        let per_query = (limit / FEATURED_QUERIES.len()).max(2);
        let mut seen: HashSet<String> = HashSet::new();
        let mut missing: BTreeSet<String> = BTreeSet::new();
        let mut hits = Vec::new();
        for query in FEATURED_QUERIES {
            let results = self.search(query, per_query).await?;
            missing.extend(results.missing_shards);
            for hit in results.hits {
                if seen.insert(hit.item.id.clone()) {
                    hits.push(hit);
                }
            }
            if hits.len() >= limit {
                break;
            }
        }
        hits.sort_by(|a, b| b.rank.total_cmp(&a.rank));
        hits.truncate(limit);
        Ok(SearchResults {
            hits,
            missing_shards: missing.into_iter().collect(),
        })
    }

    pub async fn suggest(&self, prefix: &str, limit: usize) -> Result<Vec<String>> {
        self.ensure_ready().await?;
        let tags = self.tags().await?;
        let needle = normalize_tag(prefix);
        let mut matches: Vec<&String> =
            tags.iter().filter(|tag| tag.starts_with(&needle)).collect();
        matches.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
        Ok(matches.into_iter().take(limit).cloned().collect())
    }

    async fn tags(&self) -> Result<Arc<Vec<String>>> {
        if let Some(tags) = self.state().tags.clone() {
            return Ok(tags);
        }
        let (base, path) = {
            let state = self.state();
            let path = state
                .manifest
                .as_ref()
                .and_then(|manifest| manifest.tags_index_path.clone());
            (state.base.clone(), path)
        };
        let Some(path) = path.filter(|path| !path.is_empty()) else {
            return Ok(Arc::new(Vec::new()));
        };
        let body = self.fetcher.get(&resolve(&base, &path)).await?;
        let file: TagsFile = serde_json::from_str(&body)?;
        let tags = Arc::new(file.tags);
        self.state().tags = Some(tags.clone());
        Ok(tags)
    }

    async fn ensure_ready(&self) -> Result<()> {
        for _ in 0..2 {
            let (base, epoch) = {
                let state = self.state();
                if state.ready {
                    return Ok(());
                }
                (state.base.clone(), state.epoch)
            };

            let manifest_body = self.fetcher.get(&resolve(&base, "manifest.json")).await?;
            let manifest: Manifest = serde_json::from_str(&manifest_body)?;

            let hosts_body = match manifest
                .hosts_path
                .as_deref()
                .filter(|path| !path.is_empty())
            {
                Some(path) => self
                    .fetcher
                    .get(&resolve(&base, path))
                    .await
                    .ok()
                    .filter(|body| serde_json::from_str::<HostsFile>(body).is_ok()),
                None => None,
            };

            let committed = {
                let mut state = self.state();
                if state.epoch != epoch {
                    false
                } else {
                    state.engine.set_manifest(manifest.clone())?;
                    match hosts_body {
                        Some(body) => state.engine.set_hosts_json(&body)?,
                        None => state.engine.use_default_hosts(),
                    }
                    state.manifest = Some(manifest);
                    state.ready = true;
                    true
                }
            };
            if committed {
                return Ok(());
            }
        }
        Err(Error::Config("catalog changed while loading".into()))
    }

    async fn load_shards(&self, paths: &[String], epoch: u64) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let base = self.state().base.clone();
        let chunk_size = self.config.max_concurrent_fetches.max(1);
        for chunk in paths.chunks(chunk_size) {
            let requests = chunk.iter().map(|path| {
                let url = resolve(&base, path);
                let path = path.clone();
                async move { (path, self.fetcher.get(&url).await) }
            });
            let responses = join_all(requests).await;
            let mut state = self.state();
            if state.epoch != epoch {
                return Ok(());
            }
            for (path, body) in responses {
                if let Ok(body) = body {
                    let _ = state.engine.insert_shard_json(&path, &body);
                }
            }
        }
        Ok(())
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
}

fn resolve(base: &str, path: &str) -> String {
    format!("{base}{}", path.trim_start_matches('/'))
}

fn normalize_base(base: &str, allow_insecure: bool) -> Result<String> {
    let trimmed = base.trim();
    if trimmed.is_empty() {
        return Err(Error::BaseUrl("empty".into()));
    }
    let mut url =
        Url::parse(trimmed).map_err(|error| Error::BaseUrl(format!("{trimmed}: {error}")))?;
    let loopback = matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("[::1]") | Some("::1")
    );
    match url.scheme() {
        "https" => {}
        "http" if allow_insecure || loopback => {}
        other => {
            return Err(Error::BaseUrl(format!(
                "scheme `{other}` is not allowed; use https or a loopback host"
            )))
        }
    }
    url.set_query(None);
    url.set_fragment(None);
    let mut normalized = url.to_string();
    if !normalized.ends_with('/') {
        normalized.push('/');
    }
    Ok(normalized)
}
