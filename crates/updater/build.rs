fn main() {
    println!("cargo:rerun-if-env-changed=OCTOSENSE_RELEASE_TAG");
}
