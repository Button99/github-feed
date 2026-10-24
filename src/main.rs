mod github;

use clap::{Parser, Subcommand};
use std::process::ExitCode;

// This is a github feed tracker.

#[derive(Parser, Debug)]
#[command(name = "feed")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand, Debug)]
enum Commands {
    Check {
        #[arg(short, long)]
        repo: Option<String>,
    },
    Publish {
        #[arg(short, long, default_value = "main")]
        branch: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Check { repo } => {
            let activity = match github::received_activity() {
                Ok(activity) => activity,
                Err(error) => {
                    eprintln!("{error}");
                    return ExitCode::FAILURE;
                }
            };
            let lines = github::activity_lines(&activity, repo.as_deref());
            if lines.is_empty() {
                println!("No matching activity found in the available feed.");
            } else {
                println!("TIME\tACTOR\tEVENT\tREPOSITORY");
                for line in lines.into_iter().take(10) {
                    println!("{line}");
                }
            }
        }

        Commands::Publish { branch } => {
            println!("Publishing branch {}", branch);
        }
    }
    ExitCode::SUCCESS
}
