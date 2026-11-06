#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output},
    time::SystemTime,
};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> anyhow::Result<Self> {
        let timestamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
        let fixture = Self(std::env::temp_dir().join(format!(
            "wt-cli-test-{}-{}",
            std::process::id(),
            timestamp.as_nanos()
        )));
        fs::create_dir_all(fixture.0.join("wt"))?;
        fixture.script(
            "git",
            r#"#!/bin/sh
case "$1" in
rev-parse) printf '/repository/.git\n';;
show-ref) exit 1;;
worktree)
    if [ "$2" = list ] && [ "$EMPTY_LIST" != yes ]; then
        printf 'worktree /worktree\000branch refs/heads/feature/foo\000\000'
    fi;;
esac
"#,
        )?;
        fixture.script(
            "herdr",
            r#"#!/bin/sh
if [ "$FAIL_OPEN" = yes ]; then
    echo 'open rejected' >&2
    exit 7
fi
if [ "$1" = pane ] && [ "$FAIL_RUN" = yes ]; then
    echo 'run rejected' >&2
    exit 8
fi
printf '{"result":{"root_pane":{"pane_id":"w7:p3"}}}'
"#,
        )?;
        fixture.config("[integration]\nauto_open = false")?;
        Ok(fixture)
    }

    fn script(&self, name: &str, contents: &str) -> anyhow::Result<()> {
        let path = self.0.join(name);
        fs::write(&path, contents)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        Ok(())
    }

    fn config(&self, contents: &str) -> anyhow::Result<()> {
        fs::write(self.0.join("wt/config.toml"), contents)?;
        Ok(())
    }

    fn run(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_wt"))
            .args(args)
            .env("PATH", &self.0)
            .env("HOME", &self.0)
            .env("XDG_CONFIG_HOME", &self.0)
            .envs(env.iter().copied())
            .output()
            .expect("Failed to run CLI")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("Failed to remove CLI fixture");
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

#[test]
fn completion_messages_use_stdout() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    for (args, expected) in [
        (
            vec!["new", "feature/foo"],
            "Created worktree for branch \"feature/foo\" at ",
        ),
        (
            vec!["open", "feature/foo"],
            "Opened worktree for branch \"feature/foo\" in herdr.\n",
        ),
        (
            vec!["remove", "feature/foo"],
            "Removed worktree for branch \"feature/foo\".\n",
        ),
        (vec!["list"], "feature/foo\n"),
    ] {
        let output = fixture.run(&args, &[]);
        assert!(output.status.success(), "{}", stderr(&output));
        assert!(stdout(&output).starts_with(expected));
        assert!(output.stderr.is_empty());
    }

    let output = fixture.run(&["list"], &[("EMPTY_LIST", "yes")]);
    assert!(output.status.success());
    assert_eq!(stdout(&output), "No worktree branches found.\n");
    fixture.config("[integration]\ncommand = 'cargo test'")?;
    let output = fixture.run(&["new", "feature/foo"], &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    let lines = stdout(&output);
    let lines: Vec<_> = lines.lines().collect();
    assert!(lines[0].starts_with("Created worktree"));
    assert!(lines[1].starts_with("Opened worktree"));
    assert!(lines[2].contains("herdr accepted the configured command"));
    assert!(output.stderr.is_empty());
    Ok(())
}

#[test]
fn errors_preserve_causes_and_partial_success() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    fixture.config("[integration]\ncommand = 'cargo test'")?;
    let output = fixture.run(&["new", "feature/foo'$(touch)"], &[("FAIL_OPEN", "yes")]);
    assert!(!output.status.success());
    assert!(stdout(&output).starts_with("Created worktree"));
    let error = stderr(&output);
    assert!(error.starts_with("error: Worktree was created, but automatic opening failed."));
    assert!(error.contains("open rejected"));
    assert!(error.contains("exit status: 7"));
    assert!(error.contains("Branch:"));
    assert!(error.contains("Path:"));
    assert!(error.contains("wt open 'feature/foo'\"'\"'$(touch)'"));

    let output = fixture.run(&["open", "feature/foo"], &[("FAIL_RUN", "yes")]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = stderr(&output);
    assert!(error.contains("Worktree was opened"));
    assert!(error.contains("run rejected"));
    assert!(error.contains("w7:p3"));

    let output = fixture.run(&["open", "missing"], &[]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("Hint: Use wt list"));

    fs::remove_file(fixture.0.join("herdr"))?;
    let output = fixture.run(&["open", "feature/foo"], &[]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("Install herdr"));
    assert!(stderr(&output).contains("PATH"));

    fixture.config("[invalid")?;
    let output = fixture.run(&["list"], &[]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = stderr(&output);
    assert!(error.contains(&fixture.0.join("wt/config.toml").display().to_string()));
    assert!(error.contains("Cause:"));
    assert!(error.contains("TOML"));
    Ok(())
}
