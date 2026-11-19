//! The command-line side of the run steps: turns parsed arguments into calls and prints what the operator needs.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use benchy::contract::Rubric;

use super::doctor::{self, Verdict};
use super::finish::{Finish, Finished};
use super::launch::Caps;
use super::layout::{self, RunId};
use super::new::{CreatedRun, NewRun};
use crate::collect;
use crate::{FinishArgs, HarnessArgs, HeadlessArgs, RunArgs};

/// Prints an error with its causes and turns it into exit code 1.
pub fn exit(result: Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

pub fn doctor(run_root: Option<PathBuf>, harness: Option<PathBuf>) -> Result<()> {
    let benchy_root = layout::benchy_root()?;
    let run_root = layout::run_root(run_root.as_deref())?;
    let checks = doctor::doctor(&benchy_root, &run_root, harness.as_deref())
        .context("failed to run the checks")?;
    for check in &checks {
        println!("{check}");
    }
    if checks.iter().any(|c| c.verdict == Verdict::Fail) {
        bail!("at least one check failed");
    }
    Ok(())
}

pub fn new(run_root: Option<PathBuf>, args: &HarnessArgs) -> Result<()> {
    let created = create(run_root, args)?;
    println!("{}", created.instructions());
    Ok(())
}

pub fn headless(run_root: Option<PathBuf>, args: &HeadlessArgs) -> Result<()> {
    let benchy_root = layout::benchy_root()?;
    let rubric = Rubric::load(&benchy_root.join("validation/rubric.toml"))
        .context("failed to read the run caps")?;
    let minutes = args
        .time_limit_minutes
        .unwrap_or(rubric.caps.wall_hours * 60.0);
    let caps = Caps {
        max_budget_usd: args.max_budget_usd.unwrap_or(rubric.caps.cost_usd),
        time_limit: Duration::from_secs_f64(minutes * 60.0),
    };
    let chain = super::chain::Chain {
        new_run: NewRun {
            manifest_path: args.harness.harness.clone(),
            benchy_root: benchy_root.clone(),
            run_root: layout::run_root(run_root.as_deref())?,
            task_pack: args.harness.task_pack.clone(),
        },
        caps,
        harness_program: crate::judge::command::program(),
        judge_program: crate::judge::command::program(),
    };
    println!(
        "launching claude -p with a cap of {} USD and {minutes} minutes",
        caps.max_budget_usd
    );
    let run_root = chain.new_run.run_root.clone();
    let done = chain.run(|_, id| score_step(&run_root, id))?;
    let launched = &done.launch;
    println!("run {}", done.id);
    println!(
        "claude exited: code {:?}, result {}, its own cost {}",
        launched.exit_code,
        launched.result_subtype.as_deref().unwrap_or("unknown"),
        launched
            .reported_cost_usd
            .map_or("unknown".to_string(), |c| format!("{c:.4} USD")),
    );
    if let Some(error) = &launched.error {
        println!("claude reported an error: {error}");
    }
    if launched.hit_a_cap() {
        println!("the run hit a cap: it is not finished and is scored as committed");
    }
    print_finished(&done.finished);
    print_scorecard(&done.card, &done.results);
    Ok(())
}

/// The scoring step of the chain: `benchy score <run-id>` in the scorer image, which writes `metrics.json`.
fn score_step(run_root: &std::path::Path, id: &RunId) -> Result<()> {
    let args = crate::score::ScoreArgs {
        run_id: Some(id.to_string()),
        workspace: None,
        reference: false,
        out: None,
        host: false,
        only: Vec::new(),
        par_out: None,
        scratch: None,
    };
    crate::score::run(Some(run_root), &args)?;
    Ok(())
}

pub fn judge(args: &RunArgs) -> Result<()> {
    let benchy_root = layout::benchy_root()?;
    let id = RunId::parse(&args.run_id)?;
    let judged =
        crate::judge::command::judge_run(&benchy_root, &id, &crate::judge::command::program())?;
    for item in &judged.items {
        println!(
            "{:<7} {:<18} level {} (spread {}) {}",
            item.language.as_str(),
            item.key,
            item.median,
            item.spread,
            item.evidence.join(", ")
        );
    }
    Ok(())
}

pub fn finish(args: &FinishArgs) -> Result<()> {
    let benchy_root = layout::benchy_root()?;
    let id = RunId::parse(&args.run.run_id)?;
    let finished = finish_run(&benchy_root, id, args.manual.clone())?;
    print_finished(&finished);
    Ok(())
}

fn create(run_root: Option<PathBuf>, args: &HarnessArgs) -> Result<CreatedRun> {
    let benchy_root = layout::benchy_root()?;
    let new_run = NewRun {
        manifest_path: args.harness.clone(),
        benchy_root,
        run_root: layout::run_root(run_root.as_deref())?,
        task_pack: args.task_pack.clone(),
    };
    new_run
        .create()
        .with_context(|| format!("failed to create a run for {}", args.harness.display()))
}

fn finish_run(
    benchy_root: &std::path::Path,
    id: RunId,
    manual: Option<PathBuf>,
) -> Result<Finished> {
    let finish = Finish {
        benchy_root: benchy_root.to_path_buf(),
        id: id.clone(),
        manual,
    };
    finish
        .run()
        .with_context(|| format!("failed to finish run {id}"))
}

fn print_finished(finished: &Finished) {
    let telemetry = &finished.telemetry;
    if let Some(done) = &finished.record.finish {
        println!(
            "head:        {}",
            done.head.as_deref().unwrap_or("nothing committed")
        );
        println!("uncommitted: {} paths", done.uncommitted.len());
    }
    println!(
        "cost:        {:.4} USD (transcripts' own total: {})",
        telemetry.cost_usd,
        telemetry
            .transcript_cost_usd
            .map_or("none".to_string(), |c| format!("{c:.4} USD"))
    );
    if let Some(gap) = collect::cost_gap(telemetry) {
        println!(
            "cost gap:    {:.2}% against the transcripts' own total",
            gap * 100.0
        );
    }
    println!(
        "time:        busy {:.0} s, wall {:.0} s, summed agents {:.0} s",
        telemetry.time.busy_seconds, telemetry.time.wall_seconds, telemetry.time.agent_seconds
    );
    println!(
        "agents:      {} sessions, {} subagents, depth {}, peak {} at once",
        telemetry.activity.sessions,
        telemetry.activity.subagents,
        telemetry.activity.max_depth,
        telemetry.activity.peak_concurrency
    );
    if !telemetry.manual_entries.is_empty() {
        println!("manual:      {}", telemetry.manual_entries.join(", "));
    }
    match (&finished.audit, &finished.audit_error) {
        (Some(audit), _) if audit.is_clean() => println!(
            "audit:       clean, {} tool calls checked",
            audit.tool_calls_checked
        ),
        (Some(audit), _) => {
            println!("audit:       {} findings to review", audit.findings.len());
            if audit.names_benchy() {
                println!(
                    "             some name benchy: a confirmed read of benchy material voids the run"
                );
            }
            for finding in &audit.findings {
                println!(
                    "  {:?} {} line {} {}: {}",
                    finding.kind, finding.file, finding.line, finding.tool, finding.evidence
                );
            }
        }
        (None, Some(error)) => println!("audit:       could not run: {error}"),
        (None, None) => {}
    }
}

pub fn report(args: &RunArgs) -> Result<()> {
    let benchy_root = layout::benchy_root()?;
    let id = RunId::parse(&args.run_id)?;
    let (card, dir) = crate::report::command::report(&benchy_root, &id)?;
    print_scorecard(&card, &dir);
    Ok(())
}

fn print_scorecard(card: &benchy::contract::Scorecard, dir: &std::path::Path) {
    let h = &card.headline;
    let shown = |v: Option<f64>| v.map_or("absent".to_string(), |v| format!("{v:.2}"));
    println!("quality:     {}", shown(h.quality));
    println!("cost:        {:.2} USD", h.cost_usd);
    println!("busy time:   {:.0} s", h.busy_seconds);
    println!("overall:     {}", shown(h.overall));
    println!("report:      {}", dir.join("report.md").display());
}
