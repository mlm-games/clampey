# catalog

This directory is the published catalog. Pages serves it as-is at
`https://mlm-games.github.io/clampey/`, which is the endpoint `DEFAULT_BASE_URL` points at.

The builder writes here:

```
manifest.json
hosts.json
tags.json
shards/*.json
```

Until `manifest.json` exists the Pages job skips the deploy, so the endpoint is never published
half-built.
