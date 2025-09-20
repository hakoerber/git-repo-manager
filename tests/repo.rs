use grm::repo::*;

mod helpers;

use helpers::*;

#[test]
fn open_empty_repo() {
    let tmpdir = init_tmpdir();
    assert!(matches!(
        WorktreeRepoHandle::open(tmpdir.path()),
        Err(Error::RepoNotFound)
    ));
    assert!(matches!(
        RepoHandle::open(tmpdir.path()),
        Err(Error::RepoNotFound)
    ));
    cleanup_tmpdir(tmpdir);
}

#[test]
fn create_repo() -> Result<(), Box<dyn std::error::Error>> {
    let tmpdir = init_tmpdir();
    let repo = RepoHandle::init(tmpdir.path(), WorktreeSetup::NoWorktree)?;
    assert!(!repo.is_bare());
    assert!(repo.is_empty()?);
    cleanup_tmpdir(tmpdir);
    Ok(())
}

#[test]
fn create_repo_with_worktree() -> Result<(), Box<dyn std::error::Error>> {
    let tmpdir = init_tmpdir();
    let repo = RepoHandle::init(tmpdir.path(), WorktreeSetup::Worktree)?;
    assert!(repo.is_bare());
    assert!(repo.is_empty()?);
    cleanup_tmpdir(tmpdir);
    Ok(())
}
