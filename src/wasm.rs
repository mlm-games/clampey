use wasm_bindgen::prelude::*;

use crate::{Engine, EngineConfig};

#[wasm_bindgen]
pub struct Catalog {
    engine: Engine,
}

#[wasm_bindgen]
impl Catalog {
    #[wasm_bindgen(constructor)]
    pub fn new(max_shard_fetches: Option<usize>) -> Result<Catalog, JsValue> {
        let mut config = EngineConfig::default();
        if let Some(value) = max_shard_fetches.filter(|value| *value > 0) {
            config.max_shard_fetches = value;
        }
        let engine = Engine::with_config(config).map_err(to_js)?;
        Ok(Catalog { engine })
    }

    pub fn ingest_manifest(&mut self, json: &str) -> Result<(), JsValue> {
        self.engine.set_manifest_json(json).map_err(to_js)
    }

    pub fn ingest_hosts(&mut self, json: &str) -> Result<(), JsValue> {
        self.engine.set_hosts_json(json).map_err(to_js)
    }

    pub fn use_default_hosts(&mut self) {
        self.engine.use_default_hosts();
    }

    pub fn ingest_shard(&mut self, path: &str, json: &str) -> Result<usize, JsValue> {
        self.engine.insert_shard_json(path, json).map_err(to_js)
    }

    pub fn plan(&self, query: &str) -> Result<JsValue, JsValue> {
        let plan = self.engine.plan(query).map_err(to_js)?;
        serde_wasm_bindgen::to_value(&plan).map_err(to_js)
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<JsValue, JsValue> {
        let results = self.engine.search(query, limit).map_err(to_js)?;
        serde_wasm_bindgen::to_value(&results).map_err(to_js)
    }

    pub fn stats(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.engine.stats()).map_err(to_js)
    }

    pub fn reset(&mut self) {
        self.engine.reset();
    }
}

#[wasm_bindgen]
pub fn prefix2(tag: &str) -> String {
    crate::tag_prefix(tag)
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn to_js(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}
