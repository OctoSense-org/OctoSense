# Vendored data

`model_catalog.json` is octos's model catalog, copied verbatim from
[octos-org/octos](https://github.com/octos-org/octos) at
`18fcd3f16e527d2b601d7ae244f056fb711bb6b8` (the rev AppCard's embedded kernel
pins; the file is identical on octos main as of 26 Sep 2026). octos embeds the
same file with `include_str!`; `src/catalog.rs` reads it the same way.

To update: copy the file from the octos rev AppCard pins, then run
`cargo test -p octosense-llm-config catalog` (every family must still map to
`registry::all()`, and the pinned entries must still hold).
