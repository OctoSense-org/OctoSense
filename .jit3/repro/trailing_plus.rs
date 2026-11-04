// relax parse gap seen in tls_codec 0.4.2 (impl_tls_vec_generic!: `struct $name<T: $($bounds + )*>`):
// a trailing `+` in a bound list is legal Rust; relax says "expected `>` (found Plus `+`)".
pub struct S<T: Clone + >(pub T);
impl<T: Clone + > S<T> { pub fn get(&self) -> T { self.0.clone() } }
#[test] fn trailing_plus_bound() { assert_eq!(S(3u8).get(), 3); }
