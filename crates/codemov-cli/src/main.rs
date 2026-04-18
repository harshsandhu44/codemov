mod commands;

use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "codemov",
    about = "Local codebase indexing and context engine",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize codemov data directory for the current repo
    Init {
        /// Repo root (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Index a repository
    Index {
        /// Repo root (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Force full re-index (ignore cached hashes)
        #[arg(long)]
        full: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show index statistics
    Stats {
        /// Repo root (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show codebase overview
    Overview {
        /// Repo root (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Search for symbols by name
    FindSymbol {
        /// Symbol name to search (exact, prefix, or substring)
        query: String,
        /// Repo root (defaults to current directory)
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show direct import dependencies and dependents for a file
    TraceImpact {
        /// File to trace (relative to repo root or absolute)
        file: PathBuf,
        /// Repo root (defaults to current directory)
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Init { path } => commands::init(&path),
        Command::Index { path, full, json } => commands::index(&path, full, json),
        Command::Stats { path, json } => commands::stats(&path, json),
        Command::Overview { path, json } => commands::overview(&path, json),
        Command::FindSymbol { query, path, json } => commands::find_symbol(&path, &query, json),
        Command::TraceImpact { file, path, json } => commands::trace_impact(&path, &file, json),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        process::exit(1);
    }
}
