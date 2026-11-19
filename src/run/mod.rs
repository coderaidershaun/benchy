//! The run lifecycle: create an isolated run for a harness, launch or hand over Claude Code, and close the run.
//! Each step is a function the command line calls, so a later step can chain them without going through the shell.

pub mod chain;
pub mod cli;
pub mod config;
pub mod doctor;
pub mod environment;
pub mod finish;
pub mod launch;
pub mod layout;
pub mod new;
pub mod processes;
pub mod record;
pub mod workspace;

use crate::collect::CollectError;
use crate::collect::manual::ManualError;
use benchy::contract::{ManifestError, PricesError, RubricError, ToolchainsError};

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("failed to {action}")]
    Io {
        action: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read JSON {path}")]
    Json {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("run id {id:?} is not a benchy run id (lowercase hex digits)")]
    BadRunId { id: String },
    #[error("no benchy checkout at or above {cwd}; run benchy from inside the checkout")]
    NotInBenchy { cwd: String },
    #[error("HOME is not set, so the default run root ~/runs is unknown; pass --run-root")]
    NoHome,
    #[error(
        "run root {run_root} and the benchy checkout {benchy_root} share a directory tree, which would put benchy in \
         reach of the run; choose a run root outside the checkout"
    )]
    RootNotApart {
        run_root: String,
        benchy_root: String,
    },
    #[error("run directory {path} already exists")]
    RunExists { path: String },
    #[error("{path} is a link or special file; the task pack must hold plain files only")]
    NotAPlainFile { path: String },
    #[error("settings file {path} must hold a JSON object")]
    SettingsNotObject { path: String },
    #[error("plugin {plugin} cannot be installed: {why}")]
    PluginSource { plugin: String, why: String },
    #[error(
        "plugin {plugin} installed at commit {installed}, but the manifest declares {declared}"
    )]
    PluginCommit {
        plugin: String,
        declared: String,
        installed: String,
    },
    #[error("`{command}` failed with {status}")]
    CommandFailed { command: String, status: String },
    #[error(
        "the run directory mentions the benchy checkout {benchy_root} in {path}; the run is not isolated"
    )]
    Leak { benchy_root: String, path: String },
    #[error(
        "{} processes of the run are still alive ({}); stop them before finishing",
        pids.len(),
        pids.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")
    )]
    StillAlive { pids: Vec<u32> },
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Prices(#[from] PricesError),
    #[error(transparent)]
    Toolchains(#[from] ToolchainsError),
    #[error(transparent)]
    Rubric(#[from] RubricError),
    #[error(
        "the telemetry could not be collected; write the numbers by hand in {manual} and run finish again"
    )]
    Collect {
        manual: String,
        #[source]
        source: CollectError,
    },
    #[error(transparent)]
    Manual(#[from] ManualError),
}

#[cfg(test)]
mod tests;
