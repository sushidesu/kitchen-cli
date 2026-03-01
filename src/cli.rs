use clap::{Parser, Subcommand};

use crate::commands;

#[derive(Parser)]
#[command(name = "kitchen", version, about = "A grab-bag utility CLI")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

impl Cli {
    pub fn run(self) {
        match self.command {
            Command::Hello(args) => args.run(),
            Command::Notify(args) => args.run(),
            Command::Repo(args) => args.run(),
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Print a greeting
    Hello(commands::hello::HelloArgs),
    /// Show a macOS notification
    Notify(commands::notify::NotifyArgs),
    /// Incrementally search git repositories
    Repo(commands::repo::RepoArgs),
}
