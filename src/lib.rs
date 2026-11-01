//! Clampey is a small, dependency-light search engine for static GIF catalogs.
//!
//! A catalog is a `manifest.json`, a `hosts.json`, an optional `tags.json` and a set of
//! two-character tag-prefix shards. The engine owns no network code: callers ingest JSON
//! they fetched themselves, or use [`CatalogClient`] with a [`Fetcher`].

mod cache;
pub mod client;
mod engine;
mod error;
mod host;
mod license;
mod model;
mod normalize;
mod rank;

#[cfg(feature = "reqwest")]
mod reqwest_fetcher;
#[cfg(feature = "wasm")]
mod wasm;

pub use client::{CatalogClient, ClientConfig, FetchError, Fetcher};
pub use engine::{Engine, EngineStats, SearchPlan, FEATURED_QUERIES};
pub use error::{Error, Result};
pub use host::{host_of, HostSet};
pub use license::{is_allowed, is_free, license_family, requires_attribution, requires_sharealike};
pub use model::{
    CatalogItem, EngineConfig, HostsFile, Manifest, SearchHit, SearchResults, ShardFile, ShardRef,
    TagsFile, SCHEMA_VERSION,
};
pub use normalize::{normalize_tag, tag_prefix, tokenize, MAX_QUERY_TOKENS, MAX_TAG_LEN};
pub use rank::{rank_item, score, WEIGHT_TAG_EXACT, WEIGHT_TAG_PREFIX, WEIGHT_TITLE};

#[cfg(feature = "reqwest")]
pub use reqwest_fetcher::{ReqwestFetcher, ReqwestFetcherBuilder};
