use std::{
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{Context, bail};

use crate::git::repository_root;

pub fn open_worktree(path: &Path) -> anyhow::Result<()> {
    let root = repository_root()?;
    let status = Command::new("herdr")
        .args(["worktree", "open", "--path"])
        .arg(path)
        .arg("--cwd")
        .arg(&root)
        .stdout(Stdio::null())
        .status()
        .with_context(|| format!("Failed to execute herdr for {}", path.display()))?;

    if !status.success() {
        bail!(
            "herdr worktree open failed for {} ({status})",
            path.display()
        );
    }

    Ok(())
}
