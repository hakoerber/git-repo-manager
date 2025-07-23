use grm::repo::*;

mod helpers;

use helpers::*;

#[test]
fn open_empty_repo() {
    let tmpdir = init_tmpdir();
    assert!(matches!(
        RepoHandle::open(tmpdir.path(), true),
        Err(Error::NotFound)
    ));
    assert!(matches!(
        RepoHandle::open(tmpdir.path(), false),
        Err(Error::NotFound)
    ));
    cleanup_tmpdir(tmpdir);
}

#[test]
fn create_repo() -> Result<(), Box<dyn std::error::Error>> {
    let tmpdir = init_tmpdir();
    let repo = RepoHandle::init(tmpdir.path(), false)?;
    assert!(!repo.is_bare());
    assert!(repo.is_empty()?);
    cleanup_tmpdir(tmpdir);
    Ok(())
}

#[test]
fn create_repo_with_worktree() -> Result<(), Box<dyn std::error::Error>> {
    let tmpdir = init_tmpdir();
    let repo = RepoHandle::init(tmpdir.path(), true)?;
    assert!(repo.is_bare());
    assert!(repo.is_empty()?);
    cleanup_tmpdir(tmpdir);
    Ok(())
}
