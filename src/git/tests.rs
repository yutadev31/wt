use super::*;
use std::{fs, time::SystemTime};

struct TestDirectory(PathBuf);

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("Failed to remove test directory");
    }
}

fn git(cwd: &Path, args: &[&str]) -> anyhow::Result<String> {
    let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    Ok(String::from_utf8(run_git(cwd, &args)?)?)
}

#[test]
fn creates_worktrees_and_reports_errors() -> anyhow::Result<()> {
    let timestamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
    let directory = TestDirectory(std::env::temp_dir().join(format!(
        "wt-test-{}-{}",
        std::process::id(),
        timestamp.as_nanos()
    )));
    let repo = directory.0.join("repo");
    let root = directory.0.join("worktrees");
    fs::create_dir_all(&repo)?;
    git(&repo, &["init", "--initial-branch=main"])?;
    git(
        &repo,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-m",
            "Initial commit",
        ],
    )?;

    let path = create_worktree_in(&repo, &root, "feat/hoge")?;
    assert_eq!(path, root.join("repo/feat__hoge"));
    assert_eq!(
        git(&path, &["branch", "--show-current"])?.trim(),
        "feat/hoge"
    );
    assert_eq!(
        git(&repo, &["rev-parse", "HEAD"])?,
        git(&path, &["rev-parse", "HEAD"])?
    );
    assert!(create_worktree_in(&repo, &root, "feat/hoge").is_err());
    assert_eq!(find_worktree_in(&repo, "feat/hoge")?, path);
    assert_eq!(find_worktree_in(&path, "main")?, repo);
    assert_eq!(repository_root_in(&path)?, repo);
    assert!(find_worktree_in(&repo, "missing").is_err());

    git(&repo, &["branch", "existing_branch"])?;
    let existing = create_worktree_in(&path, &root, "existing_branch")?;
    assert_eq!(existing, root.join("repo/existing_ubranch"));
    assert_eq!(find_worktree_in(&path, "existing_branch")?, existing);
    assert_eq!(
        git(&existing, &["branch", "--show-current"])?.trim(),
        "existing_branch"
    );

    for invalid in ["", "../escape", "-bad", "bad name"] {
        assert!(create_worktree_in(&repo, &root, invalid).is_err());
    }

    assert!(create_worktree_in(&directory.0, &root, "feature").is_err());
    Ok(())
}
