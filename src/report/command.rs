//! `benchy report <run-id>`: reads a run's operator data, aggregates it and writes the three result files.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use benchy::contract::{
    HarnessManifest, MEASURED_FILE, MeasuredScores, Prices, Rubric, Scorecard, Telemetry,
};
use serde::de::DeserializeOwned;

use super::{ReportInputs, write};
use crate::aggregate::{Inputs, Review, aggregate};
use crate::judge::{JUDGED_FILE, JudgedScores};
use crate::run::layout::{OperatorDir, RunId};
use crate::run::record::RunRecord;

/// The task pack carries no version number yet, so reports name it like this until one is added.
pub const TASK_VERSION_UNSET: &str = "unversioned";

/// Aggregates the run and writes `results/<label>/<run-id>/`. Returns the scorecard and the directory written.
///
/// # Errors
///
/// Fails when the measured scores, telemetry, run record or manifest are missing or do not parse.
pub fn report(benchy_root: &Path, id: &RunId) -> Result<(Scorecard, PathBuf)> {
    let operator = OperatorDir::new(benchy_root, id);
    let rubric = Rubric::load(&benchy_root.join("validation/rubric.toml"))?;
    let prices = Prices::load(&benchy_root.join("validation/prices.toml")).ok();
    let measured: MeasuredScores = read_json(&operator.dir.join(MEASURED_FILE))
        .context("the run has no measured scores; run `benchy score` first")?;
    let judge_path = operator.dir.join(JUDGED_FILE);
    let judged: Option<JudgedScores> = judge_path
        .is_file()
        .then(|| read_json(&judge_path))
        .transpose()?;
    let telemetry: Telemetry = read_json(&operator.telemetry())
        .context("the run has no telemetry; run `benchy finish` first")?;
    let record = RunRecord::load(&operator.record())?;
    let manifest = HarnessManifest::load(&operator.manifest())?;
    let review_path = operator.dir.join("review.toml");
    let review: Option<Review> = if review_path.is_file() {
        let text = fs::read_to_string(&review_path)?;
        Some(
            toml::from_str(&text)
                .with_context(|| format!("failed to parse {}", review_path.display()))?,
        )
    } else {
        None
    };
    let task_version = fs::read_to_string(benchy_root.join("task/VERSION"))
        .map(|t| t.trim().to_string())
        .unwrap_or_else(|_| TASK_VERSION_UNSET.to_string());
    let card = aggregate(&Inputs {
        rubric: &rubric,
        measured: &measured,
        judged: judged.as_ref(),
        telemetry: &telemetry,
        manifest: &manifest,
        record: &record,
        review: review.as_ref(),
        task_version: &task_version,
    });
    let inputs = ReportInputs {
        card: &card,
        rubric: &rubric,
        judged: judged.as_ref(),
        prices: prices.as_ref(),
    };
    let dir =
        write(&benchy_root.join("results"), &inputs).context("failed to write the result files")?;
    Ok((card, dir))
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
}
