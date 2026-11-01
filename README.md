# clampey

Offline-first search engine for static, openly licensed GIF catalogs.

A catalog is a folder of JSON: `manifest.json`, `hosts.json`, `tags.json` and two-character
tag-prefix shards under `shards/`. Clampey loads the shards a query actually needs, ranks in
memory, and filters out anything whose media host is not allowlisted. No API key, no server
logic, no proprietary network.

Use it from Rust, or compile the same crate to `wasm32` for browsers.

## Install

```toml
[dependencies]
clampey = "0.1"
```

Optional features:

| Feature   | Adds                                            |
| --------- | ----------------------------------------------- |
| `reqwest` | `ReqwestFetcher`, a ready-made HTTP fetcher     |
| `wasm`    | `wasm-bindgen` bindings                         |
| `cli`     | the `clampey` binary                            |

## Default endpoint

`https://mlm-games.github.io/clampey/` is the free, key-less catalog endpoint published from this
repository. It is a plain directory of JSON: `manifest.json`, `hosts.json`, `tags.json` and
`shards/`. `CatalogClient::public(fetcher)` points at it.

Any other static host works identically, so an app can expose a base-URL setting and never depend
on that one server.

```rust
let client = CatalogClient::public(ReqwestFetcher::new()?)?;
```

## Search a catalog over HTTP

```rust
use clampey::{CatalogClient, ReqwestFetcher};

let fetcher = ReqwestFetcher::builder()
    .user_agent("my-app/1.0 (https://example.org; me@example.org)")
    .build()?;
let client = CatalogClient::new(fetcher, "https://example.github.io/catalog/")?;

let results = client.search("cat", 24).await?;
for hit in results.hits {
    println!("{:.2} {} {} {}", hit.rank, hit.item.title, hit.item.license, hit.item.url);
}
```

The client fetches `manifest.json` and `hosts.json` once, then only the shards a query maps to.
Base URLs must be `https`, except loopback hosts during development.

## Search without a network

```rust
use clampey::Engine;

let mut engine = Engine::new();
engine.set_manifest_json(&std::fs::read_to_string("catalog/manifest.json")?)?;
engine.set_hosts_json(&std::fs::read_to_string("catalog/hosts.json")?)?;
engine.insert_shard_json("shards/ca.json", &std::fs::read_to_string("catalog/shards/ca.json")?)?;

let results = engine.search("cat", 24)?;
```

`Engine::plan` tells you which shards a query needs before you load them, so a lazy loader
only reads what it uses. `SearchResults::missing_shards` is non-empty when the answer is partial.

## Command line

```bash
cargo install clampey --features cli

clampey validate --dir ./catalog
clampey search cat --dir ./catalog --limit 10
clampey search cat --base https://example.github.io/catalog/
clampey bench cat --dir ./catalog
```

`validate` checks shard completeness, duplicate ids, license allowlist and host allowlist. It is
the gate to run before publishing a catalog.

## Guarantees

- Items whose `url` or `preview_url` host is outside `hosts.json` are never returned.
- Items with a non-free license are rejected by `validate`; the engine only reports what it is given.
- `SearchResults` is sorted deterministically by rank, then title, then id.
- A shard that cannot be fetched or parsed leaves the answer partial and is listed in
  `missing_shards`; it never fails the whole query.
- Shard keys, tag normalization and ranking weights are specified in [SPEC.md](SPEC.md) and locked
  by `fixtures/prefix_vectors.json`, so a second implementation can be checked against this one.

## License

MIT.
