use tempdir::TempDir;

pub fn init_tmpdir() -> TempDir {
    let tmp_dir = TempDir::new("grm-test").unwrap();
    println!("Temporary directory: {}", tmp_dir.path().display());
    tmp_dir
}

pub fn cleanup_tmpdir(tempdir: TempDir) {
    tempdir.close().unwrap();
}
