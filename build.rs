fn main() {
    if let Ok(v) = std::env::var("GRM_RELEASE_VERSION") {
        println!("cargo:rustc-env=CARGO_PKG_VERSION={}", v);
    }
    println!("cargo:rerun-if-env-changed=GRM_RELEASE_VERSION");
}
