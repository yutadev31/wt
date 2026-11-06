use std::{path::Path, process::Command};

use anyhow::{Context, bail};
use serde::Deserialize;

use crate::git::repository_root;

pub fn open_worktree(path: &Path) -> anyhow::Result<()> {
    open_worktree_with_command(path, None)
}

#[derive(Deserialize)]
struct OpenResponse {
    result: OpenResult,
}

#[derive(Deserialize)]
struct OpenResult {
    root_pane: Pane,
}

#[derive(Deserialize)]
struct Pane {
    pane_id: String,
}

pub fn open_worktree_with_command(path: &Path, command: Option<&str>) -> anyhow::Result<()> {
    let root = repository_root()?;
    open_worktree_in(path, &root, command, Path::new("herdr"))
}

fn open_worktree_in(
    path: &Path,
    root: &Path,
    command: Option<&str>,
    executable: &Path,
) -> anyhow::Result<()> {
    let output = Command::new(executable)
        .args(["worktree", "open", "--path"])
        .arg(path)
        .arg("--cwd")
        .arg(root)
        .output()
        .with_context(|| format!("Failed to execute herdr for {}", path.display()))?;

    if !output.status.success() {
        bail!(
            "herdr worktree open failed for {} ({}): {}",
            path.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim_end()
        );
    }

    if let Some(command) = command {
        let response: OpenResponse = serde_json::from_slice(&output.stdout)
            .context("Failed to parse herdr worktree open response")?;

        anyhow::ensure!(
            !response.result.root_pane.pane_id.is_empty(),
            "herdr worktree open returned an empty pane ID"
        );

        let output = Command::new(executable)
            .args(["pane", "run", &response.result.root_pane.pane_id, command])
            .output()
            .with_context(|| format!("Failed to execute herdr pane run for {}", path.display()))?;

        if !output.status.success() {
            bail!(
                "herdr pane run failed for {} ({}): {}",
                path.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim_end()
            );
        }
    }

    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, time::SystemTime};

    struct TestDirectory(PathBuf);

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("Failed to remove test directory");
        }
    }

    #[test]
    fn opens_then_runs_in_returned_pane_and_reports_failures() -> anyhow::Result<()> {
        let timestamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
        let directory = TestDirectory(std::env::temp_dir().join(format!(
            "wt-herdr-test-{}-{}",
            std::process::id(),
            timestamp.as_nanos()
        )));
        fs::create_dir_all(&directory.0)?;
        let executable = directory.0.join("herdr");
        fs::write(
            &executable,
            r##"#!/bin/sh
printf '%s\n' "$@" >> "$0.log"
if [ "$1" = worktree ]; then
    if [ -f "$0.fail-open" ]; then
        echo 'open rejected' >&2
        exit 1
    fi
    cat "$0.response"
elif [ -f "$0.fail-run" ]; then
    echo 'run rejected' >&2
    exit 1
fi
"##,
        )?;
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))?;
        let response = executable.with_extension("response");
        let log = executable.with_extension("log");
        fs::write(&response, r#"{"result":{"root_pane":{"pane_id":"w7:p3"}}}"#)?;
        let path = Path::new("/worktree with spaces");
        let root = Path::new("/repository");
        let command = "printf '%s' 'hello world' && cargo test";
        open_worktree_in(path, root, Some(command), &executable)?;
        assert_eq!(
            fs::read_to_string(&log)?,
            format!(
                "worktree\nopen\n--path\n/worktree with spaces\n--cwd\n/repository\npane\nrun\nw7:p3\n{command}\n"
            )
        );

        fs::write(&log, "")?;
        fs::write(&response, "invalid JSON")?;
        open_worktree_in(path, root, None, &executable)?;
        assert!(!fs::read_to_string(&log)?.contains("pane\nrun"));
        assert!(open_worktree_in(path, root, Some(command), &executable).is_err());

        fs::write(&log, "")?;
        fs::write(executable.with_extension("fail-open"), "")?;
        let error = open_worktree_in(path, root, Some(command), &executable).unwrap_err();
        assert!(error.to_string().contains("open rejected"));
        assert!(!fs::read_to_string(&log)?.contains("pane\nrun"));

        fs::remove_file(executable.with_extension("fail-open"))?;
        fs::write(&response, r#"{"result":{"root_pane":{"pane_id":"w7:p3"}}}"#)?;
        fs::write(executable.with_extension("fail-run"), "")?;
        let error = open_worktree_in(path, root, Some(command), &executable).unwrap_err();
        assert!(error.to_string().contains("run rejected"));

        Ok(())
    }
}
