// tls_codec 0.4.2 tls_vec.rs:415 shape: a derive on a struct whose macro-built bound list ends in `+`.
macro_rules! mk {
    ($name:ident $(, $bounds:ident)*) => {
        #[derive(Debug, Eq, PartialEq)]
        pub struct $name<T: $($bounds + )*> { pub v: T }
    };
}
mk!(S1, Copy);
#[test] fn derive_trailing_plus() { assert_eq!(S1 { v: 3u8 }, S1 { v: 3u8 }); }
