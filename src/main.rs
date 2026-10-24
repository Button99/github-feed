mod cache;
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
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
        /// Fetch activity even when the five-minute cache is fresh.
        #[arg(long)]
        refresh: bool,
    },
    Publish {
        #[arg(short, long, default_value = "main")]
        branch: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Check {
            repo,
            limit,
            refresh,
        } => {
            let activity = match github::received_activity(*refresh) {
                Ok(activity) => activity,
                Err(error) => {
                    eprintln!("{error}");
                    return ExitCode::FAILURE;
                }
            };
            eprintln!(
                "Feed cache: {}; saved_at={}",
                activity.source, activity.saved_at
            );
            if let Some(warning) = &activity.warning {
                eprintln!("{warning}");
            }
            let lines = github::activity_lines(&activity.text, repo.as_deref());
            if lines.is_empty() {
                println!("No matching activity found in the available feed.");
            } else {
                println!("TIME\tACTOR\tEVENT\tREPOSITORY");
                for line in lines.into_iter().take(*limit) {
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
