use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clampey::{
    is_free, normalize_tag, tag_prefix, CatalogClient, CatalogItem, Engine, EngineConfig, HostSet,
    HostsFile, Manifest, ReqwestFetcher, SearchResults, ShardFile, TagsFile, FEATURED_QUERIES,
};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "clampey", version, about = "Search open-licensed GIF catalogs")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Search a catalog
    Search {
        query: String,
        /// Read the catalog from a local directory
        #[arg(long, conflicts_with = "base")]
        dir: Option<PathBuf>,
        /// Read the catalog from a static HTTP endpoint
        #[arg(long)]
        base: Option<String>,
        #[arg(long, default_value_t = 24)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Show the curated default selection
    Featured {
        #[arg(long, conflicts_with = "base")]
        dir: Option<PathBuf>,
        #[arg(long)]
        base: Option<String>,
        #[arg(long, default_value_t = 24)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Suggest tags for a prefix
    Suggest {
        prefix: String,
        #[arg(long, conflicts_with = "base")]
        dir: Option<PathBuf>,
        #[arg(long)]
        base: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Check a built catalog for structural and license errors
    Validate {
        #[arg(long)]
        dir: PathBuf,
    },
    /// Measure query latency against a local catalog
    Bench {
        query: String,
        #[arg(long)]
        dir: PathBuf,
        #[arg(long, default_value_t = 200)]
        iterations: usize,
    },
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Search {
            query,
            dir,
            base,
            limit,
            json,
        } => {
            let results = search(&query, dir.as_deref(), base.as_deref(), limit).await?;
            print_results(&results, json)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Featured {
            dir,
            base,
            limit,
            json,
        } => {
            let results = featured(dir.as_deref(), base.as_deref(), limit).await?;
            print_results(&results, json)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Suggest {
            prefix,
            dir,
            base,
            limit,
        } => {
            let tags = match (dir.as_deref(), base.as_deref()) {
                (Some(dir), None) => offline_suggest(dir, &prefix, limit)?,
                (None, Some(base)) => remote(base).await?.suggest(&prefix, limit).await?,
                (Some(_), Some(_)) => bail!("use either --dir or --base, not both"),
                (None, None) => bail!("one of --dir or --base is required"),
            };
            for tag in tags {
                println!("{tag}");
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Validate { dir } => validate(&dir),
        Command::Bench {
            query,
            dir,
            iterations,
        } => {
            bench(&dir, &query, iterations)?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

async fn search(
    query: &str,
    dir: Option<&Path>,
    base: Option<&str>,
    limit: usize,
) -> Result<SearchResults> {
    match (dir, base) {
        (Some(dir), None) => Ok(load_offline(dir)?.search(query, limit)?),
        (None, Some(base)) => Ok(remote(base).await?.search(query, limit).await?),
        (Some(_), Some(_)) => bail!("use either --dir or --base, not both"),
        (None, None) => bail!("one of --dir or --base is required"),
    }
}

async fn featured(dir: Option<&Path>, base: Option<&str>, limit: usize) -> Result<SearchResults> {
    match (dir, base) {
        (Some(dir), None) => {
            let engine = load_offline(dir)?;
            let mut seen = std::collections::HashSet::new();
            let mut missing: BTreeSet<String> = BTreeSet::new();
            let mut hits = Vec::new();
            for query in FEATURED_QUERIES {
                let results = engine.search(query, 4)?;
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
        (None, Some(base)) => Ok(remote(base).await?.featured(limit).await?),
        (Some(_), Some(_)) => bail!("use either --dir or --base, not both"),
        (None, None) => bail!("one of --dir or --base is required"),
    }
}

async fn remote(base: &str) -> Result<CatalogClient<ReqwestFetcher>> {
    let fetcher = ReqwestFetcher::builder()
        .user_agent(format!("clampey/{} (cli)", env!("CARGO_PKG_VERSION")))
        .build()?;
    Ok(CatalogClient::new(fetcher, base)?)
}

fn load_offline(dir: &Path) -> Result<Engine> {
    let manifest: Manifest =
        serde_json::from_str(&read(dir.join("manifest.json"))?).context("parsing manifest.json")?;
    let mut engine = Engine::with_config(EngineConfig {
        max_cached_shards: usize::MAX,
        max_cached_items: usize::MAX,
        ..EngineConfig::default()
    })?;
    engine.set_manifest(manifest.clone())?;
    match manifest
        .hosts_path
        .as_deref()
        .filter(|path| !path.is_empty())
    {
        Some(path) => engine.set_hosts_json(&read(dir.join(path))?)?,
        None => engine.use_default_hosts(),
    }
    for shard in &manifest.shards {
        if shard.path.is_empty() {
            continue;
        }
        engine.insert_shard_json(&shard.path, &read(dir.join(&shard.path))?)?;
    }
    Ok(engine)
}

fn offline_suggest(dir: &Path, prefix: &str, limit: usize) -> Result<Vec<String>> {
    let manifest: Manifest =
        serde_json::from_str(&read(dir.join("manifest.json"))?).context("parsing manifest.json")?;
    let Some(path) = manifest.tags_index_path.filter(|path| !path.is_empty()) else {
        return Ok(Vec::new());
    };
    let file: TagsFile = serde_json::from_str(&read(dir.join(path))?)?;
    let needle = normalize_tag(prefix);
    let mut tags: Vec<String> = file
        .tags
        .into_iter()
        .filter(|tag| tag.starts_with(&needle))
        .collect();
    tags.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    tags.truncate(limit);
    Ok(tags)
}

fn validate(dir: &Path) -> Result<ExitCode> {
    let mut errors: Vec<String> = Vec::new();
    let manifest: Manifest =
        serde_json::from_str(&read(dir.join("manifest.json"))?).context("parsing manifest.json")?;

    if !manifest.schema_version.starts_with("1.") {
        errors.push(format!(
            "manifest: unsupported schema_version `{}`",
            manifest.schema_version
        ));
    }
    if manifest.shards.len() != manifest.shard_count as usize {
        errors.push(format!(
            "manifest: shard_count is {} but {} shards are listed",
            manifest.shard_count,
            manifest.shards.len()
        ));
    }

    let hosts: Option<HostSet> = match manifest.hosts_path.as_deref().filter(|p| !p.is_empty()) {
        Some(path) => {
            let hosts_path = dir.join(path);
            if !hosts_path.exists() {
                errors.push(format!("manifest: hosts_path `{path}` does not exist"));
                None
            } else {
                match serde_json::from_str::<HostsFile>(&read(hosts_path)?) {
                    Ok(file) => Some(file.hosts),
                    Err(error) => {
                        errors.push(format!("hosts: {error}"));
                        None
                    }
                }
            }
        }
        None => {
            errors.push("manifest: hosts_path is missing".to_string());
            None
        }
    };

    let mut shard_ids: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut items: HashMap<String, String> = HashMap::new();

    for shard_ref in &manifest.shards {
        if shard_ref.path.is_empty() {
            errors.push(format!(
                "manifest: shard `{}` has no path",
                shard_ref.prefix
            ));
            continue;
        }
        if shard_ref.prefix != tag_prefix(&shard_ref.prefix) {
            errors.push(format!(
                "manifest: shard prefix `{}` is not a canonical tag prefix",
                shard_ref.prefix
            ));
        }
        let shard_path = dir.join(&shard_ref.path);
        let body = match fs::read_to_string(&shard_path) {
            Ok(body) => body,
            Err(error) => {
                errors.push(format!("{}: {error}", shard_ref.path));
                continue;
            }
        };
        let shard: ShardFile = match serde_json::from_str(&body) {
            Ok(shard) => shard,
            Err(error) => {
                errors.push(format!("{}: {error}", shard_ref.path));
                continue;
            }
        };
        if shard.prefix != shard_ref.prefix {
            errors.push(format!(
                "{}: prefix `{}` does not match manifest `{}`",
                shard_ref.path, shard.prefix, shard_ref.prefix
            ));
        }
        if shard.items.len() != shard_ref.item_count as usize {
            errors.push(format!(
                "{}: {} items but manifest says {}",
                shard_ref.path,
                shard.items.len(),
                shard_ref.item_count
            ));
        }

        let mut ids = Vec::with_capacity(shard.items.len());
        for item in &shard.items {
            if item.id.is_empty() || item.url.is_empty() {
                errors.push(format!("{}: item with an empty id or url", shard_ref.path));
                continue;
            }
            if !is_free(&item.license) {
                errors.push(format!(
                    "{}: `{}` has non-free license `{}`",
                    shard_ref.path, item.id, item.license
                ));
            }
            if let Some(hosts) = &hosts {
                if !hosts.allows_url(&item.url, true) {
                    errors.push(format!(
                        "{}: `{}` media host is not allowlisted",
                        shard_ref.path, item.id
                    ));
                }
                if !hosts.allows_url(&item.preview_url, true) {
                    errors.push(format!(
                        "{}: `{}` preview host is not allowlisted",
                        shard_ref.path, item.id
                    ));
                }
            }
            let fingerprint = serde_json::to_string(item)?;
            match items.get(&item.id) {
                Some(existing) if existing != &fingerprint => {
                    errors.push(format!("`{}` differs between shards", item.id))
                }
                Some(_) => {}
                None => {
                    items.insert(item.id.clone(), fingerprint);
                }
            }
            ids.push(item.id.clone());
        }
        shard_ids.insert(shard.prefix.clone(), ids);
    }

    if manifest.item_count as usize != items.len() {
        errors.push(format!(
            "manifest: item_count is {} but {} unique items were found",
            manifest.item_count,
            items.len()
        ));
    }

    for (id, fingerprint) in &items {
        let item: CatalogItem = serde_json::from_str(fingerprint)?;
        if item.tags.is_empty() {
            errors.push(format!("`{id}` has no tags"));
        }
        for tag in &item.tags {
            let prefix = tag_prefix(tag);
            match shard_ids.get(&prefix) {
                None => errors.push(format!(
                    "`{id}` tag `{tag}` needs shard `{prefix}`, which does not exist"
                )),
                Some(ids) if !ids.iter().any(|candidate| candidate == id) => errors.push(format!(
                    "`{id}` is missing from shard `{prefix}` for tag `{tag}`"
                )),
                Some(_) => {}
            }
        }
    }

    for error in &errors {
        eprintln!("error: {error}");
    }
    if errors.is_empty() {
        println!(
            "ok: {} items across {} shards",
            items.len(),
            manifest.shards.len()
        );
        Ok(ExitCode::SUCCESS)
    } else {
        println!("{} error(s)", errors.len());
        Ok(ExitCode::FAILURE)
    }
}

fn bench(dir: &Path, query: &str, iterations: usize) -> Result<()> {
    let engine = load_offline(dir)?;
    let warmup = engine.search(query, 24)?;
    let iterations = iterations.max(1);
    let mut hits = 0;
    let start = Instant::now();
    for _ in 0..iterations {
        hits += engine.search(query, 24)?.hits.len();
    }
    let elapsed = start.elapsed().as_secs_f64();
    let per_query = elapsed / iterations as f64;
    println!("query: {query}");
    println!("iterations: {iterations}");
    println!("hits per query: {}", hits / iterations);
    println!("mean: {:.1} us/query", per_query * 1e6);
    println!("throughput: {:.0} queries/s", 1.0 / per_query);
    if warmup.hits.is_empty() {
        println!("note: the query returned no hits");
    }
    Ok(())
}

fn print_results(results: &SearchResults, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(results)?);
        return Ok(());
    }
    if results.hits.is_empty() {
        println!("no results");
    }
    for hit in &results.hits {
        println!(
            "{:>5.2}  {:<12}  {:<40}  {}",
            hit.rank,
            hit.item.license,
            truncate(&hit.item.title, 40),
            hit.item.url
        );
    }
    if !results.missing_shards.is_empty() {
        eprintln!(
            "warning: {} shard(s) were not available: {}",
            results.missing_shards.len(),
            results.missing_shards.join(", ")
        );
    }
    Ok(())
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut out: String = text.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn read(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}
