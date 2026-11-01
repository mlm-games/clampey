use crate::model::CatalogItem;
use crate::normalize::normalize_tag;

pub const WEIGHT_TAG_EXACT: f64 = 3.0;
pub const WEIGHT_TAG_PREFIX: f64 = 2.0;
pub const WEIGHT_TITLE: f64 = 1.0;

const MIN_PREFIX_TOKEN_LEN: usize = 2;

pub fn score(tags: &[String], title: &str, base: f64, tokens: &[String]) -> Option<f64> {
    if tokens.is_empty() {
        return None;
    }
    let mut extra = 0.0;
    for token in tokens {
        extra += tag_weight(tags, token);
        if !token.is_empty() && title.contains(token.as_str()) {
            extra += WEIGHT_TITLE;
        }
    }
    if extra > 0.0 {
        Some(base + extra)
    } else {
        None
    }
}

pub fn rank_item(item: &CatalogItem, tokens: &[String]) -> Option<f64> {
    let tags: Vec<String> = item
        .tags
        .iter()
        .map(|tag| normalize_tag(tag))
        .filter(|tag| !tag.is_empty())
        .collect();
    score(&tags, &normalize_tag(&item.title), item.score, tokens)
}

fn tag_weight(tags: &[String], token: &str) -> f64 {
    let allow_prefix = token.chars().count() >= MIN_PREFIX_TOKEN_LEN;
    let mut best = 0.0;
    for tag in tags {
        let weight = if tag == token || has_word(tag, |word| word == token) {
            WEIGHT_TAG_EXACT
        } else if allow_prefix
            && (tag.starts_with(token)
                || token.starts_with(tag.as_str())
                || has_word(tag, |word| word.starts_with(token)))
        {
            WEIGHT_TAG_PREFIX
        } else {
            0.0
        };
        if weight > best {
            best = weight;
        }
        if best >= WEIGHT_TAG_EXACT {
            break;
        }
    }
    best
}

fn has_word(tag: &str, predicate: impl Fn(&str) -> bool) -> bool {
    tag.split(' ').any(predicate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn exact_beats_prefix_beats_title() {
        let tokens = vec!["cat".to_string()];
        let exact = score(&tags(&["cat"]), "", 0.0, &tokens).unwrap();
        let prefix = score(&tags(&["catalogue"]), "", 0.0, &tokens).unwrap();
        let title = score(&tags(&["dog"]), "a cat", 0.0, &tokens).unwrap();
        assert_eq!(exact, WEIGHT_TAG_EXACT);
        assert_eq!(prefix, WEIGHT_TAG_PREFIX);
        assert_eq!(title, WEIGHT_TITLE);
    }

    #[test]
    fn multi_word_tags_match_each_word() {
        let tokens = vec!["up".to_string()];
        assert_eq!(
            score(&tags(&["thumbs up"]), "", 0.0, &tokens),
            Some(WEIGHT_TAG_EXACT)
        );
    }

    #[test]
    fn no_overlap_is_no_match() {
        let tokens = vec!["cat".to_string()];
        assert_eq!(score(&tags(&["car"]), "car", 0.9, &tokens), None);
    }
}
