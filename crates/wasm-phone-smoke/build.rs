use std::{env, fs, path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Cannot read fixture source identity"
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn main() {
    let manifest = env::var_os("CARGO_MANIFEST_DIR").unwrap();
    let root = Path::new(&manifest).ancestors().nth(2).unwrap();
    let revision = git(root, &["rev-parse", "HEAD"]);
    let runtime = root.join(".sources/makepad");
    let lock: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("runtime-patches.lock.json")).unwrap()).unwrap();
    let tree = git(&runtime, &["write-tree"]);
    assert_eq!(
        Some(tree.as_str()),
        lock["makepad"]["tree"].as_str(),
        "Prepare the pinned runtime before building the fixture"
    );
    let runtime_clean = Command::new("git")
        .args(["diff", "--quiet", "--"])
        .current_dir(&runtime)
        .status()
        .unwrap()
        .success();
    let dirty = !runtime_clean
        || !git(root, &["status", "--porcelain", "--untracked-files=normal"]).is_empty();
    println!("cargo:rustc-env=WASM_FIXTURE_REVISION={revision}");
    println!("cargo:rustc-env=WASM_FIXTURE_RUNTIME_TREE={tree}");
    println!("cargo:rustc-env=WASM_FIXTURE_DIRTY={dirty}");
    for path in [
        "src",
        "build.rs",
        "../shell/examples/connected_support/mod.rs",
        "../../runtime-patches.lock.json",
        "../../.sources/makepad/widgets/src/splash_host.rs",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    for name in ["HEAD", "index"] {
        println!(
            "cargo:rerun-if-changed={}",
            git(root, &["rev-parse", "--git-path", name])
        );
    }
    let symbolic = Command::new("git")
        .args(["symbolic-ref", "-q", "HEAD"])
        .current_dir(root)
        .output()
        .unwrap();
    if symbolic.status.success() {
        let name = String::from_utf8(symbolic.stdout).unwrap();
        println!(
            "cargo:rerun-if-changed={}",
            git(root, &["rev-parse", "--git-path", name.trim()])
        );
    }
}
