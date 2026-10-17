use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgGroup, Parser, Subcommand, ValueEnum};

use reqtool::evidence::{self, Outcome, Recorded, RunRequest, Timestamp};
use reqtool::project::Project;
use reqtool::report;
use reqtool::schema::Method;

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
    /// Show status: of everything, of one requirement by ID, of a governing
    /// requirement, or of the requirements allocated to an item path such as
    /// `ferrowasp_flight::rc_receiver`.
    Check { target: Option<String> },
    /// Record one run of a test or procedure: a result, at the current
    /// fingerprint, for every requirement entry that names it.
    #[command(group(ArgGroup::new("method").required(true).args(["test", "procedure"])))]
    Record {
        /// The test that ran, as a crate-qualified path.
        #[arg(long)]
        test: Option<String>,
        /// The procedure that ran, as `path#heading`.
        #[arg(long)]
        procedure: Option<String>,
        /// The board, for mcu, bench and flight evidence.
        #[arg(long)]
        board: Option<String>,
        /// The run's result.
        #[arg(long, value_enum)]
        result: RunOutcome,
        /// A requirement that failed in a run that otherwise passed.
        #[arg(long = "failed", value_name = "REQUIREMENT")]
        failed: Vec<String>,
        #[arg(long)]
        note: Option<String>,
        /// An earlier run this one supersedes. Only on the user's explicit
        /// ruling, given with --ruling.
        #[arg(long = "supersedes", value_name = "RUN_ID")]
        supersedes: Vec<String>,
        #[arg(long)]
        ruling: Option<String>,
        /// Print the record without writing it.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum RunOutcome {
    Pass,
    Fail,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let repository = match cli.repository.or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| reqtool::find_repository(&cwd))
    }) {
        Some(repository) => repository,
        None => return fail("no repository found; pass --repository"),
    };
    let (project, problems) = match Project::load(&repository) {
        Ok(loaded) => loaded,
        Err(message) => return fail(&message),
    };
    for problem in &problems {
        eprintln!("{problem}");
    }
    if !problems.is_empty() {
        eprintln!("{} problem(s)", problems.len());
        return ExitCode::FAILURE;
    }
    let result = match cli.command {
        Command::Validate => Ok(format!(
            "{} requirements valid\n",
            project.requirements.len()
        )),
        Command::Check { target } => recorded(&project).and_then(|evidence| match target {
            None => Ok(report::summary(&project, &evidence)),
            Some(target) => report::check(&project, &target, &evidence),
        }),
        Command::Record {
            test,
            procedure,
            board,
            result,
            failed,
            note,
            supersedes,
            ruling,
            dry_run,
        } => {
            let method = match (test, procedure) {
                (Some(path), None) => Method::Test(path),
                (None, Some(reference)) => Method::Procedure(reference),
                _ => unreachable!("clap requires exactly one method"),
            };
            let request = RunRequest {
                method,
                board: board.as_deref(),
                outcome: match result {
                    RunOutcome::Pass => Outcome::Pass,
                    RunOutcome::Fail => Outcome::Fail,
                },
                failed: &failed,
                note: note.as_deref(),
                supersedes: &supersedes,
                ruling: ruling.as_deref(),
            };
            let now = Timestamp::now();
            evidence::build(&project, &request, now).and_then(|record| {
                if dry_run {
                    serde_json::to_string_pretty(&record)
                        .map(|text| text + "\n")
                        .map_err(|e| e.to_string())
                } else {
                    evidence::write(&repository, &record, &now).map(|path| {
                        format!(
                            "recorded {} ({} results)\n",
                            path.strip_prefix(&repository).unwrap_or(&path).display(),
                            record.results.len()
                        )
                    })
                }
            })
        }
    };
    match result {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(message) => fail(&message),
    }
}

fn recorded(project: &Project) -> Result<Recorded, String> {
    let records = evidence::load(&project.repository)?;
    Recorded::new(project, &records)
}

fn fail(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode::FAILURE
}
