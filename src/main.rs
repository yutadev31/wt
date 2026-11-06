use clap::{Parser, Subcommand};
use wt::{
    git::{create_worktree, find_worktree, list_worktree_branches, remove_worktree},
    integration::herdr::open_worktree,
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

    match cli.command {
        CliCommand::New { branch } => {
            let path = create_worktree(&branch)?;
            println!("{}", path.display());
            open_worktree(&path)?;
        }
        CliCommand::Open { branch } => {
            let path = find_worktree(&branch)?;
            open_worktree(&path)?;
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
