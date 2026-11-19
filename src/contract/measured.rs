//! The seam between scoring and aggregation: what `benchy score` writes as `metrics.json` for one run.
//! Points here are before correctness scaling and gates; aggregation applies those once.

use serde::{Deserialize, Serialize};

use super::points::Language;
use super::scorecard::{BuildOutcome, GroupRate, MetricScore};

/// The `metrics.json` format version this build writes.
pub const MEASURED_SCHEMA: u32 = 1;

/// The file name of the measured scores inside the operator data of a run.
pub const MEASURED_FILE: &str = "metrics.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasuredScores {
    pub schema: u32,
    /// Hash of the committed workspace that was scored.
    pub workspace: String,
    /// Digest of the scorer image, when scoring ran in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scorer_image: Option<String>,
    /// One entry for each language, in the order of [`Language::ALL`].
    pub languages: Vec<MeasuredLanguage>,
}

/// The measured metrics of one language, with points before correctness scaling and gates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasuredLanguage {
    pub language: Language,
    pub build: BuildOutcome,
    /// Mean over feature groups of the share of golden sessions passed, from 0 to 1.
    pub golden_pass_rate: f64,
    /// One entry for each measured metric of the rubric, keyed as the rubric keys it.
    pub metrics: Vec<MetricScore>,
    /// Golden sessions passed in each feature group, sorted by group name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<GroupRate>,
    /// Share of the non-blank lines of production files that hold only a comment, from 0 to 1.
    /// Aggregation uses it for the comment-density cap of the rubric.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment_density: Option<f64>,
    /// What an operator must know to read the numbers, such as a tool that could not run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_scores_survive_a_json_round_trip() {
        let scores = MeasuredScores {
            schema: MEASURED_SCHEMA,
            workspace: "abc123".to_owned(),
            scorer_image: None,
            languages: vec![MeasuredLanguage {
                language: Language::Go,
                build: BuildOutcome::Built,
                golden_pass_rate: 0.75,
                metrics: vec![MetricScore {
                    key: "A.golden".to_owned(),
                    raw: Some(0.75),
                    points: 20.25,
                    max: 27.0,
                }],
                groups: vec![GroupRate {
                    group: "limit-orders".to_owned(),
                    passed: 15,
                    total: 20,
                }],
                comment_density: Some(0.12),
                notes: vec!["staticcheck could not run".to_owned()],
            }],
        };

        let json = serde_json::to_string(&scores).expect("measured scores serialise");
        let read: MeasuredScores = serde_json::from_str(&json).expect("measured scores parse");

        assert_eq!(read, scores);
    }
}
