use std::{
    env::home_dir,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use anyhow::{Context, bail};

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

fn check_output(output: Output) -> anyhow::Result<Vec<u8>> {
    if !output.status.success() {
        bail!(
            "git failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim_end()
        );
    }

    Ok(output.stdout)
}

fn run_git(cwd: &Path, args: &[&OsStr]) -> anyhow::Result<Vec<u8>> {
    check_output(git_output(cwd, args)?)
        .with_context(|| format!("git {args:?} failed in {}", cwd.display()))
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
        _ => bail!(
            "Failed to check branch {branch:?} in {} ({}): {}",
            repo.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim_end()
        ),
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
    .context("Invalid branch name")?;

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

    bail!("No worktree found for branch {branch:?}")
}

pub fn find_worktree(branch: &str) -> anyhow::Result<PathBuf> {
    find_worktree_in(Path::new("."), branch)
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
mod tests;
