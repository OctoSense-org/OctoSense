# The contract's fixture corpus

Real manifests and packages, each with what every `octosense-app-contract`
`1.x` must make of it (OctoSense ADR 0005, section 2). `tests/fixtures.rs`
loads every directory here and compares.

**The corpus is append-only within `1.x`.** Add fixtures; never change or
remove one, and never edit a key of an existing `expected.json`. A change
that would need it is not additive and belongs in `2.0`.

## A fixture

```
<name>/
  package/          the package: manifest.json and its files, as shipped
  expected.json     what the contract makes of it
```

`expected.json`:

| Key | Meaning |
| --- | --- |
| `source` | Where the package comes from. |
| `limits` | The host ceilings it is resolved under: `system` (`HostLimits::system()`), `default` (`HostLimits::default()`) or `unsigned` (the default, with `require_signature` off). |
| `digest` | `digest_dir(package)`. |
| `ignored_fields` | `AppManifest::ignored_fields()`: empty unless the manifest was written for a newer `1.x`. |
| `signing_blake3` | `bundle_digest` of the manifest's canonical signing bytes. |
| `entry` | `script` when the package has a `main.splash` (`script_source`), else `card`. |
| `policy` | The resolved `AppPolicy` as JSON. A later `1.x` may add fields; every pinned one must hold. |
| `refused` | Instead of `ignored_fields`, `signing_blake3`, `entry` and `policy`: the refusal, from parse, `admit_digest` or `resolve`. A later `1.x` may say more, never something else. |

Packages whose source manifest had an empty or placeholder
`integrity.bundle_blake3` carry the digest filled in, as the tools that pack
them do. Packages from other repositories (OctoSense, Rinx) are
Apache-2.0, like this one.

## Adding one

1. Create `<name>/package/` and `<name>/expected.json` with `source` and
   `limits` only.
2. Run `APP_CONTRACT_BLESS=1 cargo test -p octosense-app-contract --test
   fixtures`. It fills in the missing keys and never changes existing ones.
3. Review the filled-in `expected.json` as carefully as code: from now on it
   is the contract.
