use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use reqtool::project::Project;
use reqtool::report;
use reqtool::status::NoEvidence;

/// FerroWasp's requirements and verification tool.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// The repository root; found from the current directory when omitted.
    #[arg(long, global = true)]
    repository: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check every requirement file against the schema, the roots, each other
    /// and the code. Exits nonzero on any problem.
    Validate,
    /// Show status: of everything, of one requirement by ID, or of the
    /// requirements allocated to an item path such as `ferrowasp_flight::rc_receiver`.
    Check { target: Option<String> },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let repository = match cli.repository.or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| reqtool::find_repository(&cwd))
    }) {
        Some(repository) => repository,
        None => {
            eprintln!("error: no repository found; pass --repository");
            return ExitCode::FAILURE;
        }
    };
    let (project, problems) = match Project::load(&repository) {
        Ok(loaded) => loaded,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };
    for problem in &problems {
        eprintln!("{problem}");
    }
    if !problems.is_empty() {
        eprintln!("{} problem(s)", problems.len());
        return ExitCode::FAILURE;
    }
    match cli.command {
        Command::Validate => {
            println!("{} requirements valid", project.requirements.len());
            ExitCode::SUCCESS
        }
        Command::Check { target: None } => {
            print!("{}", report::summary(&project, &NoEvidence));
            ExitCode::SUCCESS
        }
        Command::Check {
            target: Some(target),
        } => match report::check(&project, &target, &NoEvidence) {
            Ok(text) => {
                print!("{text}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        },
    }
}
