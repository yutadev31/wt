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
    let error = find_worktree_in(&repo, "missing").unwrap_err();
    assert!(error.to_string().contains("missing"));
    assert!(error.to_string().contains("wt list"));

    git(&repo, &["branch", "existing_branch"])?;
    let existing = create_worktree_in(&path, &root, "existing_branch")?;
    assert_eq!(existing, root.join("repo/existing_ubranch"));
    assert_eq!(find_worktree_in(&path, "existing_branch")?, existing);
    assert_eq!(
        git(&existing, &["branch", "--show-current"])?.trim(),
        "existing_branch"
    );

    git(&repo, &["branch", "without-worktree"])?;
    let detached = directory.0.join("detached");
    git(
        &repo,
        &["worktree", "add", "--detach", detached.to_str().unwrap()],
    )?;

    let mut branches = list_worktree_branches_in(&repo)?;
    branches.sort();
    assert_eq!(branches, ["existing_branch", "feat/hoge", "main"]);
    let mut branches_from_worktree = list_worktree_branches_in(&path)?;
    branches_from_worktree.sort();
    assert_eq!(branches_from_worktree, branches);
    assert!(list_worktree_branches_in(&directory.0).is_err());

    assert!(remove_worktree_in(&repo, "missing").is_err());
    assert!(remove_worktree_in(&path, "main").is_err());
    assert!(repo.exists());

    fs::write(existing.join("untracked.txt"), "keep this file")?;
    assert!(remove_worktree_in(&repo, "existing_branch").is_err());
    assert_eq!(
        fs::read_to_string(existing.join("untracked.txt"))?,
        "keep this file"
    );
    fs::remove_file(existing.join("untracked.txt"))?;

    git(&repo, &["worktree", "lock", existing.to_str().unwrap()])?;
    assert!(remove_worktree_in(&repo, "existing_branch").is_err());
    assert!(existing.exists());
    git(&repo, &["worktree", "unlock", existing.to_str().unwrap()])?;

    remove_worktree_in(&path, "existing_branch")?;
    assert!(!existing.exists());
    assert!(find_worktree_in(&repo, "existing_branch").is_err());
    assert!(branch_exists(&repo, "existing_branch")?);

    remove_worktree_in(&repo, "feat/hoge")?;
    assert!(!path.exists());
    assert_eq!(list_worktree_branches_in(&repo)?, ["main"]);
    assert!(branch_exists(&repo, "feat/hoge")?);

    for invalid in ["", "../escape", "-bad", "bad name"] {
        let error = create_worktree_in(&repo, &root, invalid).unwrap_err();
        assert!(error.to_string().contains(&format!("{invalid:?}")));
        assert!(format!("{error:#}").contains("check-ref-format"));
    }

    assert!(create_worktree_in(&directory.0, &root, "feature").is_err());
    Ok(())
}
