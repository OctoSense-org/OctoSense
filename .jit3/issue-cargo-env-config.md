# relax: cargo's `[env]` table (.cargo/config.toml) is not applied to `env!()`

## What happens
A workspace whose `.cargo/config.toml` sets a compile-time variable

```toml
[env]
OCTOSENSE_WORKSPACE = { value = ".sources", relative = true }
```

and reads it with `include_str!(concat!(env!("OCTOSENSE_WORKSPACE"), "/…"))` builds with cargo, but `relax check` reports at every such site

```
internal: `include_str!`: expected a literal, found `compile_error ! ("`concat!`: expected a literal, found `compile_error ! ("`env!`: environment variable `OCTOSENSE_WORKSPACE` not defined at compile time
```

`crates.rs` takes the per-crate environment from cargo's unit graph (per-crate vars, build-script `rustc-env`) and falls back to the process environment; cargo's `[env]` config table (with its `relative = true` resolution against the config file's directory) is not read. Exporting the variable in the process environment before `relax check` is the workaround (14 errors → 0 here).

## Suggestion
Read `[env]` from the cargo config files cargo would consult for the workspace (`cargo config get env` prints the merged table), resolving `relative = true` entries against the config's parent of `.cargo`, and apply them with the same precedence cargo uses (`force = true` wins over the process environment, otherwise the process environment wins).
