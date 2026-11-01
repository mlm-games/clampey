# Clampey catalog and search specification

Version `1.0.0`. Everything here is normative; the golden vectors in `fixtures/` are the
executable form of the rules.

## 1. Catalog layout

```
manifest.json
hosts.json
tags.json
shards/<prefix>.json
```

`manifest.json` declares `schema_version`, `shards[].prefix`, `shards[].path`, `shards[].item_count`,
`hosts_path` and `tags_index_path`. Clients must accept unknown fields and must reject a
`schema_version` that does not start with `1.`.

## 2. Tag normalization

Applied to item tags, titles and query tokens:

1. Trim, lowercase (Unicode).
2. Keep Unicode alphanumerics and `_ - + #`. Drop everything else, including punctuation.
3. Collapse runs of whitespace to a single space; drop leading and trailing spaces.
4. Truncate to 64 characters.

## 3. Shard key

`prefix(tag)`:

1. Normalize the tag.
2. Keep only ASCII `a-z` and `0-9`.
3. Take the first two characters, pad with `_` to length 2.
4. If nothing remains, the prefix is `__`.

An item is written into the shard of every tag it carries. A shard may therefore contain items
that do not match a query that fetched it; ranking discards them.

## 4. Ranking

For a query tokenized into `tokens` (normalized, deduplicated, at most 8):

```
rank = item.score
for token in tokens:
    rank += max over tags of tag_weight(tag, token)
    if token is a substring of normalized title: rank += 1
```

`tag_weight` for a normalized tag and a token:

| Condition                                                     | Weight |
| ------------------------------------------------------------- | ------ |
| tag equals token, or one of the tag's words equals the token  | 3      |
| tag starts with token, or token starts with tag (token ≥ 2)   | 2      |
| one of the tag's words starts with token (token ≥ 2)          | 2      |
| otherwise                                                     | 0      |

An item is a candidate only if at least one token produced a non-zero contribution; `item.score`
alone never qualifies an item. Candidates with `rank <= min_rank` (default `0.5`) are dropped.
Results are sorted by `rank` descending, then `title` ascending, then `id` ascending.

Candidate selection is prefix-based (§3): a query fetches the shard of each token's prefix, so a
word in the middle of a multi-word tag can raise the rank of an item that another token already
retrieved, but cannot retrieve the item on its own.

## 5. Media hosts

`hosts.json` is `{"hosts": ["example.org", ...]}`. A URL is servable when:

- its scheme is `https` (or `http` when the engine is configured to allow it),
- it has no username or password,
- its host equals an allowlisted host or is a subdomain of one.

`url` and `preview_url` are both checked. Unparseable URLs are never served.

## 6. Licenses

Machine form: `cc0`, `pdm`, `cc-by-X.Y`, `cc-by-sa-X.Y`. A license family is the license with a
trailing `-X.Y` removed. The free families are `cc0`, `pdm`, `cc-by` and `cc-by-sa`; anything
with `nc` or `nd` is not free and must not appear in a catalog.

## 7. Versioning

`schema_version` changes only on breaking format changes. Ranking and shard-key changes are
breaking for parity purposes and must update `fixtures/prefix_vectors.json`.
