use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::engine::ItemDoc;

#[derive(Debug)]
pub(crate) struct ShardCache {
    entries: HashMap<String, Entry>,
    items: usize,
    max_shards: usize,
    max_items: usize,
    clock: AtomicU64,
}

#[derive(Debug)]
struct Entry {
    docs: Vec<ItemDoc>,
    last_used: AtomicU64,
}

impl ShardCache {
    pub(crate) fn new(max_shards: usize, max_items: usize) -> Self {
        Self {
            entries: HashMap::new(),
            items: 0,
            max_shards: max_shards.max(1),
            max_items: max_items.max(1),
            clock: AtomicU64::new(0),
        }
    }

    pub(crate) fn get(&self, path: &str) -> Option<&[ItemDoc]> {
        let entry = self.entries.get(path)?;
        entry.last_used.store(self.tick(), Ordering::Relaxed);
        Some(&entry.docs)
    }

    pub(crate) fn contains(&self, path: &str) -> bool {
        self.entries.contains_key(path)
    }

    pub(crate) fn insert(&mut self, path: String, docs: Vec<ItemDoc>) {
        let tick = self.tick();
        self.items += docs.len();
        if let Some(previous) = self.entries.insert(
            path,
            Entry {
                docs,
                last_used: AtomicU64::new(tick),
            },
        ) {
            self.items = self.items.saturating_sub(previous.docs.len());
        }
        self.evict();
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.items = 0;
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn item_count(&self) -> usize {
        self.items
    }

    fn tick(&self) -> u64 {
        self.clock.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn evict(&mut self) {
        while self.entries.len() > self.max_shards || self.items > self.max_items {
            let victim = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used.load(Ordering::Relaxed))
                .map(|(path, _)| path.clone());
            let Some(victim) = victim else {
                break;
            };
            if let Some(removed) = self.entries.remove(&victim) {
                self.items = self.items.saturating_sub(removed.docs.len());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CatalogItem;

    fn doc(id: &str) -> ItemDoc {
        ItemDoc::new(
            CatalogItem {
                id: id.to_string(),
                url: format!("https://upload.wikimedia.org/{id}.gif"),
                ..CatalogItem::default()
            },
            true,
        )
        .unwrap()
    }

    #[test]
    fn evicts_least_recently_used() {
        let mut cache = ShardCache::new(2, 100);
        cache.insert("a".into(), vec![doc("a")]);
        cache.insert("b".into(), vec![doc("b")]);
        assert!(cache.get("a").is_some());
        cache.insert("c".into(), vec![doc("c")]);
        assert!(cache.contains("a"));
        assert!(!cache.contains("b"));
        assert!(cache.contains("c"));
    }

    #[test]
    fn evicts_by_item_budget() {
        let mut cache = ShardCache::new(10, 3);
        cache.insert("a".into(), vec![doc("a1"), doc("a2")]);
        cache.insert("b".into(), vec![doc("b1"), doc("b2")]);
        assert_eq!(cache.item_count(), 2);
        assert!(cache.contains("b"));
    }
}
