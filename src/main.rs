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

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::load()?;

    match cli.command {
        CliCommand::New { branch } => {
            let path = create_worktree(&branch)?;
            println!("{}", path.display());

            if config.integration.auto_open {
                open_worktree(&path, &config.integration).with_context(|| {
                    format!(
                        "Worktree for branch {branch:?} was created at {}, but automatic opening failed; retry with `wt open {branch:?}`",
                        path.display()
                    )
                })?;
            }
        }
        CliCommand::Open { branch } => {
            let path = find_worktree(&branch)?;
            open_worktree(&path, &config.integration)?;
        }
        CliCommand::Remove { branch } => remove_worktree(&branch)?,
        CliCommand::List => {
            for branch in list_worktree_branches()? {
                println!("{branch}");
            }
        }
    }

    Ok(())
}
