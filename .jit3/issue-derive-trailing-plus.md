# relax: a `#[derive]` on a struct whose macro-built bound list ends in `+` fails to parse ("expected `>` (found Plus `+`)")

`makepad check` of a program that depends on `tls_codec` 0.4.2 (via sigstore-verify) stops at `tls_codec/src/tls_vec.rs:897`:
`expected `>` (found Plus `+`)`. rustc accepts the crate.

The crate's `impl_tls_vec_generic!` builds its generics by repetition, `pub struct $name<T: $($bounds + )*> { … }`, so the expansion
has a trailing `+` in the bound list (`T: Zeroize + >`), which is legal. The plain struct parses; the same struct under a
`#[derive(Eq, Debug)]` does not, so the derive expansion's re-parse of the generics rejects the trailing `+`.

Repro (relax test, upstream work 60abec8ba):

```rust
macro_rules! mk {
    ($name:ident $(, $bounds:ident)*) => {
        #[derive(Debug, Eq, PartialEq)]
        pub struct $name<T: $($bounds + )*> { pub v: T }
    };
}
mk!(S1, Copy);
#[test] fn derive_trailing_plus() { assert_eq!(S1 { v: 3u8 }, S1 { v: 3u8 }); }
```

`relax test repro.rs` → `repro.rs:8:1: expected `>` (found Plus `+`)`; `rustc --test repro.rs` → 1 passed.
Without the derive, or with `struct S<T: Clone + >(T);` written directly, relax passes.
