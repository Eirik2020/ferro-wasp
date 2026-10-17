use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgGroup, Parser, Subcommand, ValueEnum};

use reqtool::evidence::{self, Outcome, Recorded, RunRequest, Timestamp};
use reqtool::import::Imported;
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
    /// Carry the old schema 1 and 2 run records over as schema 3 evidence,
    /// re-fingerprinted at each record's own revision. Originals stay.
    Import {
        /// Show what would be imported without writing.
        #[arg(long)]
        dry_run: bool,
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
    if let Command::Import { dry_run } = &cli.command {
        return import(&project, *dry_run);
    }
    if let Command::Test {
        requirements,
        dry_run,
    } = &cli.command
    {
        return test(&project, requirements, *dry_run);
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
        Command::Test { .. } | Command::Import { .. } => unreachable!("handled above"),
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

fn import(project: &Project, dry_run: bool) -> ExitCode {
    let plans = match reqtool::import::plan(project) {
        Ok(plans) => plans,
        Err(message) => return fail(&message),
    };
    let relative = |path: &std::path::Path| {
        path.strip_prefix(&project.repository)
            .unwrap_or(path)
            .display()
            .to_string()
    };
    if dry_run {
        for plan in &plans {
            match &plan.outcome {
                Ok(Some(record)) => {
                    println!(
                        "would import {} ({} results)",
                        relative(&plan.source),
                        record.results.len()
                    );
                    for result in &record.results {
                        println!(
                            "    {} {}: {}",
                            result.requirement,
                            result.level.name(),
                            result.fingerprint
                        );
                    }
                }
                Ok(None) => {}
                Err(why) => println!("would skip   {}: {why}", relative(&plan.source)),
            }
        }
        return ExitCode::SUCCESS;
    }
    for (source, imported) in reqtool::import::write(project, plans) {
        match imported {
            Imported::Written { path, results } => {
                println!(
                    "imported {} -> {} ({results} results)",
                    relative(&source),
                    relative(&path)
                );
            }
            Imported::Present(path) => println!("present  {}", relative(&path)),
            Imported::Skipped(why) => println!("skipped  {}: {why}", relative(&source)),
        }
    }
    ExitCode::SUCCESS
}

fn recorded(project: &Project) -> Result<Recorded, String> {
    let records = evidence::load(&project.repository)?;
    Recorded::new(project, &records)
}

fn fail(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode::FAILURE
}
