# Slice 019b deployment evidence

Deployment from merged main `237639d`, with unchanged verified code hash
`a75ee70b9b3b51021ca00853109e71ff8c222ca46ad1f91c0937976fabf88e0a`.

- [API build](build-api.log) and [staged Web build](build-web.log).
- [Public tunnel check](tunnel.log).
- [Authenticated and public smoke results](smoke-results.json).
- [Running artifact identifiers and SHA-256](deployed-artifacts.json).
- [Six public Web assets matched to local build](asset-checks.json).

Full [release record](../../../../tasks/SLICE_019b_RELEASE.md) includes
procedure, cleanup, limitations and recovery. The smoke helper serialized no
credentials or private CRM content. [Artifact manifest](artifact-sha256.json)
covers every other file in this directory.
