//! `benchy judge <run-id>`: unpacks the committed workspace outside the checkout, keeps only the code and
//! `DELIVERY.md`, judges it and writes `judge.json` in the run's operator data.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use benchy::contract::{Language, Rubric};

use super::{JUDGED_FILE, Judge, JudgeRubric, JudgedScores};
use crate::run::layout::{OperatorDir, RunId};
use crate::run::record::write_json;

/// How often one judgement is asked again after an invalid or uncited reply.
pub const MAX_ATTEMPTS: u32 = 3;

/// What the judge may see of the workspace. Everything else, the task pack included, is removed first.
const VISIBLE: [&str; 4] = ["rust", "go", "csharp", "DELIVERY.md"];

/// # Errors
///
/// Fails when the run has no workspace snapshot, the judge is not set up, or a judgement stays invalid.
pub fn judge_run(benchy_root: &Path, id: &RunId, program: &Path) -> Result<JudgedScores> {
    let operator = OperatorDir::new(benchy_root, id);
    let scoring = Rubric::load(&benchy_root.join("validation/rubric.toml"))?;
    let rubric = JudgeRubric::load(&benchy_root.join("validation/judge"), &scoring)?;
    let tarball = operator.workspace_tarball();
    if !tarball.is_file() {
        bail!(
            "{} is missing; run `benchy finish {id}` first",
            tarball.display()
        );
    }
    // The judge works outside the checkout, under a neutral name, so it cannot read scores or the harness name.
    let workspace = std::env::temp_dir().join(format!("judge-{id}"));
    unpack(&tarball, &workspace)?;
    let judge = Judge {
        program: program.to_path_buf(),
        model: scoring.judge.model.clone(),
        samples: scoring.judge.samples,
        max_attempts: MAX_ATTEMPTS,
        scale_max: scoring.judge.scale_max,
        rubric,
    };
    let judged = judge.judge(&workspace, &Language::ALL);
    let _ = fs::remove_dir_all(&workspace);
    let judged = judged.context("the judge gave no valid scores")?;
    write_json(&operator.dir.join(JUDGED_FILE), &judged)?;
    Ok(judged)
}

/// The judge program: `BENCHY_CLAUDE` when set, else `claude` on the PATH.
pub fn program() -> PathBuf {
    std::env::var_os("BENCHY_CLAUDE").map_or_else(|| PathBuf::from("claude"), PathBuf::from)
}

fn unpack(tarball: &Path, workspace: &PathBuf) -> Result<()> {
    if workspace.exists() {
        fs::remove_dir_all(workspace)
            .with_context(|| format!("failed to clear {}", workspace.display()))?;
    }
    fs::create_dir_all(workspace)?;
    let status = Command::new("tar")
        .arg("-xf")
        .arg(tarball)
        .arg("-C")
        .arg(workspace)
        .status()
        .context("failed to start tar")?;
    if !status.success() {
        bail!("tar could not unpack {}: {status}", tarball.display());
    }
    for entry in fs::read_dir(workspace)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if VISIBLE.contains(&name.as_str()) {
            continue;
        }
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}
