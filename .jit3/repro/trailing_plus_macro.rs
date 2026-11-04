// tls_codec 0.4.2 shape: a macro repetition that leaves a trailing `+` in a bound list.
macro_rules! mk {
    ($name:ident $(, $bounds:ident)*) => {
        pub struct $name<T: $($bounds + )*> { pub v: T }
        impl<T: $($bounds + )* Clone> Clone for $name<T> { fn clone(&self) -> Self { $name { v: self.v.clone() } } }
        impl<T: $($bounds + )*> $name<T> { pub fn get(&self) -> &T { &self.v } }
    };
}
mk!(S0);
mk!(S1, Copy);
#[test] fn macro_trailing_plus() { assert_eq!(*S1 { v: 3u8 }.clone().get(), 3); assert_eq!(*S0 { v: 1u8 }.get(), 1); }
