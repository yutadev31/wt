use clap::{Parser, Subcommand};
use wt::git::create_worktree;

#[derive(Parser)]
struct Cli {
    #[clap(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    New { branch: String },
    Remove,
    List,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        CliCommand::New { branch } => {
            let path = create_worktree(&branch)?;
            println!("{}", path.display());
        }
        CliCommand::Remove => {}
        CliCommand::List => {}
    }

    Ok(())
}
