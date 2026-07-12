use std::fs;

fn main() {
    let version = fs::read_to_string("version.txt")
        .expect("version.txt not found at crate root")
        .trim()
        .to_string();
    assert!(!version.is_empty(), "version.txt is empty");
    println!("cargo:rustc-env=G13_VERSION={version}");
    println!("cargo:rerun-if-changed=version.txt");
}
