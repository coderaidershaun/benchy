//! Shared contracts that every benchy step reads or writes: the harness manifest, rubric, prices, toolchains,
//! telemetry, scorecard and the engine protocol's words. Each type here is a file format on disk.

mod manifest;
mod measured;
mod points;
mod prices;
mod protocol;
mod rubric;
mod rubric_tables;
mod scorecard;
mod telemetry;
mod toolchains;

pub use manifest::{
    AuthMode, ClaudeCode, ComparabilityKey, ExternalSpend, HarnessManifest, IsolationLevel,
    KICKOFF_PROMPT, Launch, MANIFEST_SCHEMA, ManifestError, PERMITTED_REPLIES, PROMPT_PLACEHOLDER,
    Plugin, ToolServer,
};
pub use measured::{MEASURED_FILE, MEASURED_SCHEMA, MeasuredLanguage, MeasuredScores};
pub use points::{CategoryId, CategoryPoints, Language};
pub use prices::{ModelPrice, Prices, PricesError};
pub use protocol::{
    CommandCode, EventKind, MAX_LINE_BYTES, MAX_ORDER_ID, MAX_SESSION_LINES, MAX_VALUE, OutReason,
    RejectCode, TimeInForce,
};
pub use rubric::{
    BuildGate, Category, CommentDensityCap, CorrectnessScaling, Efficiency, JudgeSettings,
    LANGUAGE_POINTS, Measurement, Metric, OverallWeights, QualityWeights, Rubric, RubricError,
    RubricStatus, RunCaps, Scale, Source,
};
pub use rubric_tables::{RubricTable, TablesError};
pub use scorecard::{
    BuildOutcome, Completion, GroupRate, Headline, LanguageScore, MetricScore, RunStatus,
    SCORECARD_SCHEMA, Scorecard, Validity, Versions,
};
pub use telemetry::{Activity, Agent, ModelTokens, RunTime, Telemetry, TelemetrySource};
pub use toolchains::{DotnetToolchain, Staticcheck, Toolchain, Toolchains, ToolchainsError};
