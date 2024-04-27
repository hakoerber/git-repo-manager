use outdir_tempdir::TempDir;

pub fn init_tmpdir() -> TempDir {
    let tmp_dir = TempDir::new().autorm();
    println!("Temporary directory: {}", tmp_dir.path().display());
    tmp_dir
}

pub fn cleanup_tmpdir(_tempdir: TempDir) {
    // cleanup on drop
}
