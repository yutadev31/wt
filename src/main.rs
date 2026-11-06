use std::{path::Path, process::ExitCode};

use anyhow::Context;
use clap::{Parser, Subcommand};
use wt::{
    config::Config,
    git::{create_worktree, find_worktree, list_worktree_branches, remove_worktree},
    integration::open_worktree,
};

#[derive(Parser)]
struct Cli {
    #[clap(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    New { branch: String },
    Open { branch: String },
    Remove { branch: String },
    List,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", render_error(&error));
            ExitCode::FAILURE
        }
    }
}

fn render_error(error: &anyhow::Error) -> String {
    let mut message = format!("error: {error}");
    for cause in error.chain().skip(1) {
        message.push_str(&format!(
            "\n  Cause: {}",
            cause.to_string().replace('\n', "\n    ")
        ));
    }

    if error.downcast_ref::<wt::git::WorktreeNotFound>().is_some() {
        message.push_str("\n  Hint: Use wt list to see available branches.");
    }

    if error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        && error
            .downcast_ref::<wt::integration::herdr::HerdrExecution>()
            .is_some()
    {
        message.push_str("\n  Hint: Install herdr and ensure its executable is available in PATH.");
    }

    if let Some(created) = error.downcast_ref::<CreatedWorktree>() {
        message.push_str(&format!(
            "\n  Branch: {:?}\n  Path: {}\n  Hint: Retry with wt open {}.",
            created.branch,
            created.path.display(),
            shell_quote(&created.branch)
        ));
    }

    message
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[derive(Debug)]
struct CreatedWorktree {
    branch: String,
    path: std::path::PathBuf,
}

impl std::fmt::Display for CreatedWorktree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Worktree was created, but automatic opening failed.")
    }
}

fn open(branch: &str, path: &Path, config: &Config) -> anyhow::Result<()> {
    open_worktree(path, &config.integration)?;
    println!("Opened worktree for branch {branch:?} in herdr.");
    if config.integration.command.is_some() {
        println!("herdr accepted the configured command for branch {branch:?}.");
    }

    Ok(())
}

fn run(cli: Cli) -> anyhow::Result<()> {
    let config = Config::load().context("Failed to load configuration.")?;

    match cli.command {
        CliCommand::New { branch } => {
            let path = create_worktree(&branch)
                .with_context(|| format!("Failed to create worktree for branch {branch:?}."))?;
            println!(
                "Created worktree for branch {branch:?} at {}.",
                path.display()
            );

            if config.integration.auto_open {
                open(&branch, &path, &config).with_context(|| CreatedWorktree { branch, path })?;
            }
        }
        CliCommand::Open { branch } => {
            let result = (|| {
                let path = find_worktree(&branch)?;
                open(&branch, &path, &config)
            })();
            result.with_context(|| format!("Failed to open worktree for branch {branch:?}."))?;
        }
        CliCommand::Remove { branch } => {
            remove_worktree(&branch)
                .with_context(|| format!("Failed to remove worktree for branch {branch:?}."))?;
            println!("Removed worktree for branch {branch:?}.");
        }
        CliCommand::List => {
            let branches = list_worktree_branches().context("Failed to list worktree branches.")?;
            if branches.is_empty() {
                println!("No worktree branches found.");
            }

            for branch in branches {
                println!("{branch}");
            }
        }
    }

    Ok(())
}
