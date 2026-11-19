//! The run record in `.runs/<run-id>/run.json`: where the run lives and what has happened to it so far.
//! Each step reads it, adds what it learned and writes it back, so later steps can carry on from any earlier one.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::RunError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    pub id: String,
    pub label: String,
    pub run_dir: PathBuf,
    pub task_pack: PathBuf,
    pub created: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch: Option<LaunchRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<FinishRecord>,
}

/// What `benchy run` saw of the headless Claude Code process.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchRecord {
    pub started: String,
    pub ended: String,
    pub max_budget_usd: f64,
    pub time_limit_seconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub hit_time_limit: bool,
    /// Claude Code's own word for how the session ended, such as `success` or `error_max_budget_usd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_subtype: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reported_cost_usd: Option<f64>,
    /// Claude Code's message when the session ended in an error, such as `Not logged in`.
    /// The subtype can say `success` even then, so this is the field to read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl LaunchRecord {
    /// A run stopped by the cost cap or the time limit is scored as committed and flagged.
    pub fn hit_a_cap(&self) -> bool {
        self.hit_time_limit || self.result_subtype.as_deref() == Some("error_max_budget_usd")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinishRecord {
    pub ended: String,
    /// The committed head of the workspace's default branch, or `None` when nothing was committed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    pub branch: String,
    /// Lines of `git status --porcelain`: work that is not committed and so is not scored.
    pub uncommitted: Vec<String>,
    /// Number of audit findings, or `None` when the transcripts could not be read for the audit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_findings: Option<u32>,
}

impl RunRecord {
    /// # Errors
    ///
    /// Fails when the file cannot be read or does not parse.
    pub fn load(path: &Path) -> Result<RunRecord, RunError> {
        let text = fs::read_to_string(path).map_err(|source| RunError::Io {
            action: format!("read run record {}", path.display()),
            source,
        })?;
        serde_json::from_str(&text).map_err(|source| RunError::Json {
            path: path.display().to_string(),
            source,
        })
    }

    /// # Errors
    ///
    /// Fails when the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), RunError> {
        write_json(path, self)
    }
}

/// # Errors
///
/// Fails when the file cannot be written.
pub fn write_json(path: &Path, value: &impl Serialize) -> Result<(), RunError> {
    let text = serde_json::to_string_pretty(value).map_err(|source| RunError::Json {
        path: path.display().to_string(),
        source,
    })?;
    fs::write(path, text + "\n").map_err(|source| RunError::Io {
        action: format!("write {}", path.display()),
        source,
    })
}
