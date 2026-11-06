use std::{
    env::home_dir,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use anyhow::Context;

use crate::process::check_output;

fn encode_branch_name(branch: &str) -> String {
    branch.replace('_', "_u").replace('/', "__")
}

fn git_output(cwd: &Path, args: &[&OsStr]) -> anyhow::Result<Output> {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("Failed to execute git {args:?} in {}", cwd.display()))
}

fn run_git(cwd: &Path, args: &[&OsStr]) -> anyhow::Result<Vec<u8>> {
    check_output(
        git_output(cwd, args)?,
        &format!("git {args:?} in {}", cwd.display()),
    )
}

fn repository_root_in(cwd: &Path) -> anyhow::Result<PathBuf> {
    let output = run_git(
        cwd,
        &[
            "rev-parse".as_ref(),
            "--path-format=absolute".as_ref(),
            "--git-common-dir".as_ref(),
        ],
    )?;

    let git_dir = String::from_utf8(output).context("Git common directory is not valid UTF-8")?;
    let git_dir = PathBuf::from(git_dir.trim_end_matches('\n'));

    git_dir
        .parent()
        .map(Path::to_path_buf)
        .context("Failed to determine repository root")
}

pub fn repository_root() -> anyhow::Result<PathBuf> {
    repository_root_in(Path::new("."))
}

fn repository_name(cwd: &Path) -> anyhow::Result<String> {
    let root = repository_root_in(cwd)?;
    let name = root
        .file_name()
        .context("Failed to determine repository name")?;

    Ok(name.to_string_lossy().into_owned())
}

fn branch_exists(repo: &Path, branch: &str) -> anyhow::Result<bool> {
    let reference = format!("refs/heads/{branch}");
    let output = git_output(
        repo,
        &[
            "show-ref".as_ref(),
            "--verify".as_ref(),
            "--quiet".as_ref(),
            reference.as_ref(),
        ],
    )?;

    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => check_output(
            output,
            &format!("Checking branch {branch:?} in {}", repo.display()),
        )
        .map(|_| true),
    }
}

fn create_worktree_in(cwd: &Path, root: &Path, branch: &str) -> anyhow::Result<PathBuf> {
    run_git(
        cwd,
        &[
            "check-ref-format".as_ref(),
            "--branch".as_ref(),
            branch.as_ref(),
        ],
    )
    .with_context(|| format!("Failed to validate branch name {branch:?}"))?;

    let repo = repository_name(cwd)?;
    let path = root.join(repo).join(encode_branch_name(branch));
    let exists = branch_exists(cwd, branch)?;
    let mut args = vec!["worktree".as_ref(), "add".as_ref()];

    if !exists {
        args.push("-b".as_ref());
        args.push(branch.as_ref());
    }

    args.extend(["--".as_ref(), path.as_os_str()]);

    if exists {
        args.push(branch.as_ref());
    }

    run_git(cwd, &args)
        .with_context(|| format!("Failed to create worktree at {}", path.display()))?;

    Ok(path)
}

pub fn create_worktree(branch: &str) -> anyhow::Result<PathBuf> {
    let root = home_dir()
        .context("Failed to get home directory")?
        .join("dev/.worktrees");

    create_worktree_in(Path::new("."), &root, branch)
}

#[derive(Debug)]
pub struct WorktreeNotFound(String);

impl std::fmt::Display for WorktreeNotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "No worktree found for branch {:?}", self.0)
    }
}

impl std::error::Error for WorktreeNotFound {}

fn find_worktree_in(cwd: &Path, branch: &str) -> anyhow::Result<PathBuf> {
    let output = run_git(
        cwd,
        &[
            "worktree".as_ref(),
            "list".as_ref(),
            "--porcelain".as_ref(),
            "-z".as_ref(),
        ],
    )?;

    let output = String::from_utf8(output).context("Worktree list is not valid UTF-8")?;
    let reference = format!("branch refs/heads/{branch}");
    let mut path = None;

    for field in output.split('\0') {
        if let Some(value) = field.strip_prefix("worktree ") {
            path = Some(PathBuf::from(value));
        } else if field == reference {
            return path.context("Missing path in git worktree list");
        } else if field.is_empty() {
            path = None;
        }
    }

    Err(WorktreeNotFound(branch.to_owned()).into())
}

pub fn find_worktree(branch: &str) -> anyhow::Result<PathBuf> {
    find_worktree_in(Path::new("."), branch)
}

fn remove_worktree_in(cwd: &Path, branch: &str) -> anyhow::Result<()> {
    let path = find_worktree_in(cwd, branch)?;
    run_git(
        cwd,
        &[
            "worktree".as_ref(),
            "remove".as_ref(),
            "--".as_ref(),
            path.as_os_str(),
        ],
    )
    .with_context(|| format!("Failed to remove worktree for branch {branch:?}"))?;

    Ok(())
}

pub fn remove_worktree(branch: &str) -> anyhow::Result<()> {
    remove_worktree_in(Path::new("."), branch)
}

fn list_worktree_branches_in(cwd: &Path) -> anyhow::Result<Vec<String>> {
    let output = run_git(
        cwd,
        &[
            "worktree".as_ref(),
            "list".as_ref(),
            "--porcelain".as_ref(),
            "-z".as_ref(),
        ],
    )?;

    let output = String::from_utf8(output).context("Worktree list is not valid UTF-8")?;

    Ok(output
        .split('\0')
        .filter_map(|field| field.strip_prefix("branch refs/heads/"))
        .map(str::to_owned)
        .collect())
}

pub fn list_worktree_branches() -> anyhow::Result<Vec<String>> {
    list_worktree_branches_in(Path::new("."))
}

#[cfg(test)]
mod tests {
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
        assert!(error.downcast_ref::<WorktreeNotFound>().is_some());

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
}
