fn main() {
    let a: Vec<_> = std::env::args().collect();
    std::fs::write(&a[2], wat::parse_file(&a[1]).unwrap()).unwrap();
}
