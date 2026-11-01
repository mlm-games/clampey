use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use clampey::{CatalogClient, FetchError, Fetcher};

const BASE: &str = "https://catalog.test/";

fn files() -> HashMap<String, String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/catalog");
    let mut map = HashMap::new();
    for entry in std::fs::read_dir(dir.join("shards")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        map.insert(
            format!("{BASE}shards/{name}"),
            std::fs::read_to_string(&path).unwrap(),
        );
    }
    for name in ["manifest.json", "hosts.json", "tags.json"] {
        map.insert(
            format!("{BASE}{name}"),
            std::fs::read_to_string(dir.join(name)).unwrap(),
        );
    }
    map
}

fn client_with(
    files: HashMap<String, String>,
    requests: Arc<Mutex<Vec<String>>>,
) -> CatalogClient<impl Fetcher> {
    let files = Arc::new(files);
    let fetcher = move |url: String| {
        let files = files.clone();
        let requests = requests.clone();
        async move {
            requests.lock().unwrap().push(url.clone());
            files
                .get(&url)
                .cloned()
                .ok_or_else(|| FetchError::status(404))
        }
    };
    CatalogClient::new(fetcher, BASE).unwrap()
}

fn client(requests: Arc<Mutex<Vec<String>>>) -> CatalogClient<impl Fetcher> {
    client_with(files(), requests)
}

#[test]
fn searches_the_static_catalog() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let client = client(requests.clone());

    let results = futures::executor::block_on(client.search("cat", 10)).unwrap();
    assert_eq!(results.hits.len(), 3);
    assert_eq!(results.hits[0].item.id, "cat-curated");
    assert!(results.missing_shards.is_empty());

    let requests = requests.lock().unwrap();
    assert!(requests.contains(&format!("{BASE}manifest.json")));
    assert!(requests.contains(&format!("{BASE}hosts.json")));
    assert!(requests.contains(&format!("{BASE}shards/ca.json")));
    assert!(!requests.contains(&format!("{BASE}shards/da.json")));
}

#[test]
fn caches_shards_between_queries() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let client = client(requests.clone());

    futures::executor::block_on(client.search("cat", 10)).unwrap();
    let after_first = requests.lock().unwrap().len();
    futures::executor::block_on(client.search("cat", 10)).unwrap();
    assert_eq!(requests.lock().unwrap().len(), after_first);
}

#[test]
fn a_failing_shard_yields_partial_results() {
    let mut files = files();
    files.remove(&format!("{BASE}shards/ca.json"));
    let client = client_with(files, Arc::new(Mutex::new(Vec::new())));

    let results = futures::executor::block_on(client.search("cat", 10)).unwrap();
    assert!(results.hits.is_empty());
    assert_eq!(results.missing_shards, ["shards/ca.json"]);
}

#[test]
fn suggests_tags_from_the_index() {
    let client = client(Arc::new(Mutex::new(Vec::new())));
    let tags = futures::executor::block_on(client.suggest("ca", 5)).unwrap();
    assert_eq!(tags, ["car", "cat"]);
}

#[test]
fn rejects_insecure_base_urls() {
    let client = client(Arc::new(Mutex::new(Vec::new())));
    assert!(client.set_base_url("http://catalog.test/").is_err());
    assert!(client.set_base_url("http://localhost:8080/").is_ok());
    assert!(client.set_base_url("ftp://catalog.test/").is_err());
}

#[test]
fn exposes_the_manifest() {
    let client = client(Arc::new(Mutex::new(Vec::new())));
    let manifest = futures::executor::block_on(client.manifest()).unwrap();
    assert_eq!(manifest.item_count, 6);
    assert_eq!(manifest.shard_count, 7);
}

#[test]
fn public_client_uses_the_default_endpoint() {
    let fetcher = |_url: String| async move { Err::<String, FetchError>(FetchError::status(404)) };
    let client = CatalogClient::public(fetcher).unwrap();
    assert_eq!(client.base_url(), clampey::DEFAULT_BASE_URL);
}
