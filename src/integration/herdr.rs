use std::{path::Path, process::Command};

use anyhow::Context;
use serde::Deserialize;

use crate::{git::repository_root, process::check_output};

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
    open_worktree_in(path, &root, command, || Command::new("herdr"))
}

fn open_worktree_in(
    path: &Path,
    root: &Path,
    command: Option<&str>,
    executable: impl Fn() -> Command,
) -> anyhow::Result<()> {
    let output = executable()
        .args(["worktree", "open", "--path"])
        .arg(path)
        .arg("--cwd")
        .arg(root)
        .output()
        .with_context(|| {
            format!("Failed to execute `herdr worktree open` for {}; ensure herdr is installed and available in PATH", path.display())
        })?;

    let stdout = check_output(
        output,
        &format!("herdr worktree open for {}", path.display()),
    )?;

    if let Some(command) = command {
        let response: OpenResponse = serde_json::from_slice(&stdout).with_context(|| {
            format!(
                "herdr worktree open succeeded for {}, but its response was invalid; the configured command was not run",
                path.display()
            )
        })?;

        anyhow::ensure!(
            !response.result.root_pane.pane_id.trim().is_empty(),
            "herdr worktree open succeeded for {}, but returned an empty pane ID; the configured command was not run",
            path.display()
        );

        let output = executable()
            .args(["pane", "run", &response.result.root_pane.pane_id, command])
            .output()
            .with_context(|| {
                format!(
                    "Worktree {} was opened, but failed to execute `herdr pane run` in pane {:?}",
                    path.display(),
                    response.result.root_pane.pane_id
                )
            })?;

        check_output(
            output,
            &format!(
                "herdr pane run in pane {:?} for already opened worktree {}",
                response.result.root_pane.pane_id,
                path.display()
            ),
        )?;
    }

    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf, time::SystemTime};

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
        let shell_command = || {
            let mut shell = Command::new("sh");
            shell.arg(&executable);
            shell
        };
        let response = executable.with_extension("response");
        let log = executable.with_extension("log");
        fs::write(&response, r#"{"result":{"root_pane":{"pane_id":"w7:p3"}}}"#)?;
        let path = Path::new("/worktree with spaces");
        let root = Path::new("/repository");
        let command = "printf '%s' 'hello world' && cargo test";
        open_worktree_in(path, root, Some(command), shell_command)?;
        assert_eq!(
            fs::read_to_string(&log)?,
            format!(
                "worktree\nopen\n--path\n/worktree with spaces\n--cwd\n/repository\npane\nrun\nw7:p3\n{command}\n"
            )
        );

        fs::write(&log, "")?;
        fs::write(&response, "invalid JSON")?;
        open_worktree_in(path, root, None, shell_command)?;
        assert!(!fs::read_to_string(&log)?.contains("pane\nrun"));
        let error = open_worktree_in(path, root, Some(command), shell_command).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("/worktree with spaces"));
        assert!(message.contains("configured command was not run"));
        assert!(message.contains("expected value"));

        for pane_id in ["", "   "] {
            fs::write(&log, "")?;
            fs::write(
                &response,
                serde_json::json!({"result": {"root_pane": {"pane_id": pane_id}}}).to_string(),
            )?;
            let error = open_worktree_in(path, root, Some(command), shell_command).unwrap_err();
            assert!(error.to_string().contains("empty pane ID"));
            assert!(!fs::read_to_string(&log)?.contains("pane\nrun"));
        }

        fs::write(&log, "")?;
        fs::write(executable.with_extension("fail-open"), "")?;
        let error = open_worktree_in(path, root, Some(command), shell_command).unwrap_err();
        assert!(error.to_string().contains("open rejected"));
        assert!(!fs::read_to_string(&log)?.contains("pane\nrun"));

        fs::remove_file(executable.with_extension("fail-open"))?;
        fs::write(&response, r#"{"result":{"root_pane":{"pane_id":"w7:p3"}}}"#)?;
        fs::write(executable.with_extension("fail-run"), "")?;
        let error = open_worktree_in(path, root, Some(command), shell_command).unwrap_err();
        assert!(error.to_string().contains("run rejected"));
        assert!(error.to_string().contains("already opened worktree"));
        assert!(error.to_string().contains("w7:p3"));

        let error = open_worktree_in(path, root, None, || {
            Command::new(directory.0.join("missing-executable"))
        })
        .unwrap_err();
        assert!(error.to_string().contains("PATH"));
        assert!(error.to_string().contains("/worktree with spaces"));
        assert!(error.downcast_ref::<std::io::Error>().is_some());

        Ok(())
    }
}
