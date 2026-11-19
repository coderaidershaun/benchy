//! The whole headless benchmark in one call: create, launch, finish, score, judge and report.
//! Each step is the same function its own subcommand calls, so a chain run and a step-by-step run agree.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use benchy::contract::Scorecard;

use super::finish::Finish;
use super::launch::{self, Caps};
use super::layout::RunId;
use super::new::NewRun;
use super::record::LaunchRecord;

/// Everything a chain run needs. The two programs are `claude` in real use and stand-ins in tests.
pub struct Chain {
    pub new_run: NewRun,
    pub caps: Caps,
    pub harness_program: PathBuf,
    pub judge_program: PathBuf,
}

/// What each step produced, for the command line to print.
pub struct ChainResult {
    pub id: RunId,
    pub launch: LaunchRecord,
    pub finished: super::finish::Finished,
    pub card: Scorecard,
    pub results: PathBuf,
}

impl Chain {
    /// Runs every step in order. `score` writes `metrics.json` for the run; it is passed in because the scorer
    /// lives in its own module.
    ///
    /// # Errors
    ///
    /// Fails at the first step that fails, naming it; the run's operator data keeps what the earlier steps wrote.
    pub fn run(&self, score: impl FnOnce(&Path, &RunId) -> Result<()>) -> Result<ChainResult> {
        let benchy_root = &self.new_run.benchy_root;
        let mut created = self.new_run.create().context("failed to create the run")?;
        let id = created.id.clone();
        let launch = launch::headless(&mut created, self.caps, &self.harness_program)
            .context("failed to run the harness")?;
        let finished = Finish {
            benchy_root: benchy_root.clone(),
            id: id.clone(),
            manual: None,
        }
        .run()
        .with_context(|| format!("failed to finish run {id}"))?;
        score(benchy_root, &id).with_context(|| format!("failed to score run {id}"))?;
        crate::judge::command::judge_run(benchy_root, &id, &self.judge_program)
            .with_context(|| format!("failed to judge run {id}"))?;
        let (card, results) = crate::report::command::report(benchy_root, &id)
            .with_context(|| format!("failed to report run {id}"))?;
        Ok(ChainResult {
            id,
            launch,
            finished,
            card,
            results,
        })
    }
}

#[cfg(test)]
mod tests;
