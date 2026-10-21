use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgGroup, Parser, Subcommand, ValueEnum};

use reqtool::evidence::{self, Outcome, Recorded, RunRequest, Timestamp};
use reqtool::project::Project;
use reqtool::report;
use reqtool::runner::{self, Ran};
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
    /// and the code, and that every `pub` item of a safety-critical crate has
    /// a requirement. Exits nonzero on any problem.
    Validate,
    /// Show status: of everything, of one requirement by ID, of a governing
    /// requirement, or of the requirements allocated to an item path such as
    /// `ferrowasp_input::rc_receiver`.
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
        /// A JSON file of how the run was done: execution, candidate,
        /// conditions, commands, artifacts (under logs/, hashed here),
        /// measurements, observations, limitations, and any triggered stop
        /// conditions. Required for mcu, bench and flight evidence.
        #[arg(long, value_name = "FILE")]
        details: Option<PathBuf>,
        /// Print the record without writing it.
        #[arg(long)]
        dry_run: bool,
    },
    /// Say whether a gate is open, and what stands in its way: every
    /// requirement entry it needs that is not verified.
    Gate {
        /// The gate, as named in project_meta/requirements/gates.yaml.
        name: Option<String>,
        /// The board, for gates with board-level requirements.
        #[arg(long)]
        board: Option<String>,
    },
    /// Run the host tests that requirements name, and record what each did.
    /// Tests that fail or do not run exit nonzero; only tests that ran are
    /// recorded.
    Test {
        /// Only these requirements' tests; every requirement's when omitted.
        requirements: Vec<String>,
        /// Run the tests and print the records without writing them.
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
    if let Command::Gate { name, board } = &cli.command {
        return gate(&project, name.as_deref(), board.as_deref());
    }
    if let Command::Test {
        requirements,
        dry_run,
    } = &cli.command
    {
        return test(&project, requirements, *dry_run);
    }
    let result = match cli.command {
        Command::Validate => {
            let unlinked = project.unlinked_critical_items();
            if unlinked.is_empty() {
                Ok(format!(
                    "{} requirements valid\n",
                    project.requirements.len()
                ))
            } else {
                for problem in &unlinked {
                    eprintln!("{problem}");
                }
                Err(format!(
                    "{} safety-critical item(s) without a requirement",
                    unlinked.len()
                ))
            }
        }
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
            details,
            dry_run,
        } => {
            let details = match details.map(|path| read_details(&path)).transpose() {
                Ok(details) => details,
                Err(message) => return fail(&message),
            };
            let method = match (test, procedure) {
                (Some(path), None) => Method::Test(path),
                (None, Some(reference)) => Method::Procedure(reference),
                _ => unreachable!("clap requires exactly one method"),
            };
            let outcome = match result {
                RunOutcome::Pass => Outcome::Pass,
                RunOutcome::Fail => Outcome::Fail,
            };
            let request = RunRequest {
                runs: vec![(method, outcome)],
                board: board.as_deref(),
                failed: &failed,
                note: note.as_deref(),
                supersedes: &supersedes,
                ruling: ruling.as_deref(),
                details,
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
        Command::Test { .. } | Command::Gate { .. } => {
            unreachable!("handled above")
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

/// Runs the named tests, then records the session: one record holding a
/// result for every test that ran, minus passes already on record at the
/// same fingerprint.
fn test(project: &Project, requirements: &[String], dry_run: bool) -> ExitCode {
    let outcomes = match runner::named_tests(project, requirements)
        .and_then(|tests| runner::run(project, &tests))
    {
        Ok(outcomes) => outcomes,
        Err(message) => return fail(&message),
    };
    let mut clean = true;
    let mut runs = Vec::new();
    for (path, ran) in &outcomes {
        println!("{:<11} {path}", ran.name());
        match ran {
            Ran::Passed => runs.push((Method::Test(path.clone()), Outcome::Pass)),
            Ran::Failed => {
                clean = false;
                runs.push((Method::Test(path.clone()), Outcome::Fail));
            }
            Ran::Ignored | Ran::NotRun => clean = false,
        }
    }
    if runs.is_empty() {
        println!("nothing ran, nothing recorded");
        return ExitCode::FAILURE;
    }
    let request = RunRequest {
        runs,
        board: None,
        failed: &[],
        note: None,
        supersedes: &[],
        ruling: None,
        details: None,
    };
    let now = Timestamp::now();
    let recorded = evidence::load(&project.repository).and_then(|existing| {
        let mut record = evidence::build(project, &request, now)?;
        evidence::drop_repeats(&mut record, &existing);
        if record.results.is_empty() {
            return Ok("nothing new to record: every pass is already on record".to_owned());
        }
        if dry_run {
            return serde_json::to_string_pretty(&record).map_err(|e| e.to_string());
        }
        let written = evidence::write(&project.repository, &record, &now)?;
        Ok(format!(
            "recorded {} ({} results)",
            written
                .strip_prefix(&project.repository)
                .unwrap_or(&written)
                .display(),
            record.results.len()
        ))
    });
    match recorded {
        Ok(text) => println!("{text}"),
        Err(message) => return fail(&message),
    }
    if clean {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Lists the gates, or checks one: exits zero only when it is open.
fn gate(project: &Project, name: Option<&str>, board: Option<&str>) -> ExitCode {
    let Some(name) = name else {
        for gate in &project.gates {
            println!("{:<16} {}", gate.name, gate.description);
        }
        return ExitCode::SUCCESS;
    };
    if let Some(board) = board
        && !project.workspace.boards.contains_key(board)
    {
        return fail(&format!("no firmware app for board {board}"));
    }
    let missing = recorded(project).and_then(|evidence| {
        reqtool::gates::check(
            &project.gates,
            name,
            board,
            &project.requirements,
            &evidence,
        )
    });
    match missing {
        Err(message) => fail(&message),
        Ok(missing) if missing.is_empty() => {
            println!(
                "gate {name} is open{}",
                board.map(|b| format!(" for {b}")).unwrap_or_default()
            );
            ExitCode::SUCCESS
        }
        Ok(missing) => {
            println!(
                "gate {name} is closed{}: {} entries are not verified",
                board.map(|b| format!(" for {b}")).unwrap_or_default(),
                missing.len()
            );
            for item in missing {
                println!(
                    "  {:<12} {:<9} {}",
                    item.requirement,
                    item.level.name(),
                    item.status.name()
                );
            }
            ExitCode::FAILURE
        }
    }
}

fn read_details(path: &std::path::Path) -> Result<reqtool::details::Details, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn recorded(project: &Project) -> Result<Recorded, String> {
    let records = evidence::load(&project.repository)?;
    Recorded::new(project, &records)
}

fn fail(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode::FAILURE
}
