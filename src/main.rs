//! The benchy command-line program: one subcommand for each step of a benchmark run.
//! Each step lives in its own module; a step that fails prints its error chain and exits with code 2.

mod aggregate;
mod audit;
mod collect;
mod judge;
mod mutate;
mod report;
mod run;
mod score;
mod selftest;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod conform;
mod r#gen;

#[derive(Debug, Parser)]
#[command(
    name = "benchy",
    version,
    about = "Benchmark Claude Code harnesses on one fixed task"
)]
struct Cli {
    /// Directory that holds the run directories. Defaults to ~/runs; keep it outside the benchy checkout.
    #[arg(long, global = true, env = run::layout::RUN_ROOT_VARIABLE)]
    run_root: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the whole benchmark headless: create the run, launch Claude Code, then finish, score, judge and report.
    Run(HeadlessArgs),
    /// Check toolchains, Docker and the Claude Code version against the pinned versions and a manifest.
    Doctor(DoctorArgs),
    /// Create a run for an interactive harness and print its directory, environment and kickoff prompt.
    New(HarnessArgs),
    /// Close a run: snapshot the committed workspace, archive transcripts, write telemetry and run the audit.
    Finish(FinishArgs),
    /// Score a finished run's workspace, a workspace folder or the reference workspace, and write metrics.json.
    Score(score::ScoreArgs),
    /// Judge a scored run's code with the fixed judge model and prompt.
    Judge(RunArgs),
    /// Write report.md, scorecard.json and scorecard.tsv for a scored and judged run.
    Report(RunArgs),
    /// Score the synthetic workspaces and check that the rubric orders them correctly.
    Selftest(selftest::SelftestArgs),
    /// Run an engine program over golden sessions and report the pass rate of each feature group.
    Conform(conform::ConformArgs),
    /// Write seeded engine input: fuzz streams, generated golden sessions and timing workloads.
    #[command(subcommand)]
    Gen(r#gen::GenCommand),
}

#[derive(Debug, clap::Args)]
struct HarnessArgs {
    /// Path to the harness manifest, such as harnesses/claude-code.toml.
    harness: PathBuf,
    /// Copy this directory into the workspace instead of task/. Only for smoke tests of benchy itself.
    #[arg(long, hide = true)]
    task_pack: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct HeadlessArgs {
    #[command(flatten)]
    harness: HarnessArgs,
    /// Cost cap in US dollars. Defaults to the cap in validation/rubric.toml.
    #[arg(long)]
    max_budget_usd: Option<f64>,
    /// Time limit in minutes. Defaults to the cap in validation/rubric.toml.
    #[arg(long)]
    time_limit_minutes: Option<f64>,
}

#[derive(Debug, clap::Args)]
struct FinishArgs {
    #[command(flatten)]
    run: RunArgs,
    /// TOML file with cost, time and agents entered by hand. Defaults to .runs/<run-id>/manual.toml when it exists.
    #[arg(long)]
    manual: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct DoctorArgs {
    /// Harness manifest whose Claude Code version and plugins to check as well.
    harness: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct RunArgs {
    /// Id of the run, as `benchy new` or `benchy run` printed it.
    run_id: String,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let run_root = cli.run_root;
    match cli.command {
        Command::Run(args) => run::cli::exit(run::cli::headless(run_root, &args)),
        Command::Doctor(args) => run::cli::exit(run::cli::doctor(run_root, args.harness)),
        Command::New(args) => run::cli::exit(run::cli::new(run_root, &args)),
        Command::Finish(args) => run::cli::exit(run::cli::finish(&args)),
        Command::Score(args) => match score::run(run_root.as_deref(), &args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => report_error(&error),
        },
        Command::Judge(args) => run::cli::exit(run::cli::judge(&args)),
        Command::Report(args) => run::cli::exit(run::cli::report(&args)),
        Command::Selftest(args) => match selftest::run(&args) {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::FAILURE,
            Err(error) => report_error(&error),
        },
        Command::Conform(args) => match conform::run(&args) {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::FAILURE,
            Err(error) => report_error(&error),
        },
        Command::Gen(command) => match r#gen::run(&command) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => report_error(&error),
        },
    }
}

fn report_error(error: &dyn std::error::Error) -> ExitCode {
    eprint!("error: {error}");
    let mut source = error.source();
    while let Some(cause) = source {
        eprint!(": {cause}");
        source = cause.source();
    }
    eprintln!();
    ExitCode::from(2)
}
