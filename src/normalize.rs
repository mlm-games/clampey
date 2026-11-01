pub const MAX_TAG_LEN: usize = 64;
pub const MAX_QUERY_TOKENS: usize = 8;

pub fn normalize_tag(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_space = false;
    for ch in input.trim().chars().flat_map(char::to_lowercase) {
        let keep = ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '+' || ch == '#';
        if keep {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(ch);
        } else if ch.is_whitespace() {
            pending_space = true;
        }
    }
    if out.chars().count() > MAX_TAG_LEN {
        out = out.chars().take(MAX_TAG_LEN).collect();
    }
    out
}

pub fn tag_prefix(tag: &str) -> String {
    let normalized = normalize_tag(tag);
    let mut prefix = String::with_capacity(2);
    for ch in normalized.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            prefix.push(ch);
            if prefix.len() == 2 {
                break;
            }
        }
    }
    while prefix.len() < 2 {
        prefix.push('_');
    }
    prefix
}

pub fn tokenize(query: &str) -> Vec<String> {
    let normalized = normalize_tag(query);
    let mut tokens: Vec<String> = Vec::new();
    for token in normalized.split(' ') {
        if token.is_empty() || tokens.iter().any(|t| t == token) {
            continue;
        }
        tokens.push(token.to_string());
        if tokens.len() == MAX_QUERY_TOKENS {
            break;
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_pads_and_falls_back() {
        assert_eq!(tag_prefix("a"), "a_");
        assert_eq!(tag_prefix(""), "__");
        assert_eq!(tag_prefix("猫"), "__");
        assert_eq!(tag_prefix("Thumbs Up"), "th");
    }

    #[test]
    fn normalize_collapses_and_truncates() {
        assert_eq!(normalize_tag("  Rock &   Roll "), "rock roll");
        assert_eq!(normalize_tag("hello.world"), "helloworld");
        assert_eq!(normalize_tag(&"a".repeat(80)).chars().count(), MAX_TAG_LEN);
    }

    #[test]
    fn tokenize_dedupes_and_caps() {
        assert_eq!(tokenize("Cat  cat DOG"), ["cat", "dog"]);
        assert_eq!(tokenize("").len(), 0);
        assert_eq!(tokenize("a b c d e f g h i j").len(), MAX_QUERY_TOKENS);
    }
}
