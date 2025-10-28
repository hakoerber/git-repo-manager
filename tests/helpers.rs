use tempfile::TempDir;

pub fn init_tmpdir() -> TempDir {
    // this is deleted when it goes out of scope
    let tmp_dir = TempDir::new().expect("Failed to create temporary directory");
    println!("Temporary directory: {}", tmp_dir.path().display());
    tmp_dir
}

pub fn cleanup_tmpdir(_tempdir: TempDir) {
    // cleanup on drop
}
