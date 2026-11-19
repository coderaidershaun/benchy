//! Telemetry of one run: cost, time, agents and activity, derived from the transcripts Claude Code writes.
//! Raw transcripts may hold private data and stay under `.runs/`; only these derived numbers are committed.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Telemetry {
    pub source: TelemetrySource,
    /// Every Claude Code version seen in the run's transcripts.
    pub claude_code_versions: Vec<String>,
    /// Cost priced with `validation/prices.toml`, tokens counted once per message id.
    pub cost_usd: f64,
    /// The transcripts' own total, kept as a cross-check of `cost_usd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_cost_usd: Option<f64>,
    /// Spend outside Claude Code that the manifest declares.
    pub external_spend_usd: f64,
    pub time: RunTime,
    pub tokens: Vec<ModelTokens>,
    pub agents: Vec<Agent>,
    pub activity: Activity,
    /// Values entered by hand because the transcripts could not give them, by field name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_entries: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TelemetrySource {
    #[default]
    Transcripts,
    /// Part of the telemetry was entered by hand; the report flags it.
    Mixed,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RunTime {
    /// The scored time: the union of intervals in which an API request or a tool was running.
    pub busy_seconds: f64,
    pub wall_seconds: f64,
    /// Busy time summed over every agent, so parallel agents count more than once.
    pub agent_seconds: f64,
    pub rate_limit_seconds: f64,
    pub human_wait_seconds: f64,
    /// RFC 3339 timestamps of the first and last transcript record.
    pub started: String,
    pub ended: String,
}

/// Tokens of one model, by cache class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ModelTokens {
    pub model: String,
    pub input: u64,
    pub output: u64,
    #[serde(default)]
    pub thinking: u64,
    pub cache_read: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
}

/// One main session or subagent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub agent_type: String,
    pub model: String,
    /// 0 for a main session, 1 for its subagents, and so on.
    pub depth: u32,
    pub tokens: u64,
    pub busy_seconds: f64,
    pub started: String,
    pub ended: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Activity {
    pub sessions: u32,
    pub subagents: u32,
    pub max_depth: u32,
    pub peak_concurrency: u32,
    pub turns: u32,
    pub tool_calls: BTreeMap<String, u32>,
    pub build_invocations: u32,
    pub test_invocations: u32,
    pub api_errors: u32,
    pub retries: u32,
    pub operator_replies: u32,
    pub premature_done_claims: u32,
}

impl Telemetry {
    /// Cost that the run spent in all, inside Claude Code and outside it.
    pub fn total_cost_usd(&self) -> f64 {
        self.cost_usd + self.external_spend_usd
    }
}
