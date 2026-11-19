//! The scorecard of one run: every raw value, score and version, as `scorecard.json` and as one `scorecard.tsv` row.
//! The TSV column order is fixed so rows of different harnesses paste side by side.

use serde::{Deserialize, Serialize};

use super::manifest::{HarnessManifest, IsolationLevel};
use super::points::{CategoryId, CategoryPoints, Language};
use super::telemetry::Telemetry;

/// The scorecard format version this build writes.
pub const SCORECARD_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scorecard {
    pub schema: u32,
    pub run_id: String,
    pub label: String,
    /// RFC 3339 time at which the run started.
    pub date: String,
    pub versions: Versions,
    pub isolation: IsolationLevel,
    pub validity: Validity,
    pub headline: Headline,
    /// One entry for each language, in the order of [`Language::ALL`].
    pub languages: Vec<LanguageScore>,
    pub telemetry: Telemetry,
    pub manifest: HarnessManifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Versions {
    pub task: String,
    pub rubric: String,
    pub judge_model: String,
    pub claude_code: String,
    pub benchy: String,
    /// Digest of the scorer image, when scoring ran in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scorer_image: Option<String>,
    /// Hash of the committed workspace that was scored.
    pub workspace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Validity {
    pub status: RunStatus,
    pub completion: Completion,
    /// Tool calls the audit flagged as reaching outside the run directory.
    pub audit_flags: u32,
    pub operator_replies: u32,
    pub uncommitted_work: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunStatus {
    Valid,
    /// The audit found flags that an operator has not reviewed yet.
    NeedsReview,
    /// A confirmed read of benchy material. The run has no score.
    Void,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Completion {
    Finished,
    CostCap,
    TimeCap,
}

/// The three headline numbers and the ones printed beneath them. All are absent for a void run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Headline {
    pub quality: Option<f64>,
    pub quality_mean: Option<f64>,
    pub quality_lowest: Option<f64>,
    pub lowest_language: Option<Language>,
    pub cost_usd: f64,
    pub busy_seconds: f64,
    /// Absent while the rubric has no pilot baselines.
    pub efficiency: Option<f64>,
    pub overall: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageScore {
    pub language: Language,
    pub build: BuildOutcome,
    pub golden_pass_rate: f64,
    /// The correctness factor applied to the scaled categories.
    pub correctness_factor: f64,
    pub total: f64,
    /// One entry for each category, in the order of [`CategoryId::ALL`].
    pub categories: Vec<CategoryPoints>,
    pub metrics: Vec<MetricScore>,
    /// Golden sessions passed in each feature group, sorted by group name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<GroupRate>,
    /// Share of the non-blank lines of production files that hold only a comment, from 0 to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment_density: Option<f64>,
}

/// The golden sessions of one feature group that an engine passed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupRate {
    /// Folder name of the group under `validation/scenarios`.
    pub group: String,
    pub passed: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildOutcome {
    Built,
    Failed,
}

/// One metric of one language: what was measured and the points it earned after scaling and caps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricScore {
    /// Metric key in the form `D.mutation`.
    pub key: String,
    /// Absent when the metric could not be measured; it then earns 0.
    pub raw: Option<f64>,
    pub points: f64,
    pub max: f64,
}

impl Scorecard {
    /// The fixed TSV header: run fields, headline, language totals, then each category of each language.
    pub fn tsv_columns() -> Vec<String> {
        let mut columns: Vec<String> = [
            "run_id",
            "label",
            "date",
            "task_version",
            "rubric_version",
            "judge_model",
            "claude_code_version",
            "model",
            "auth",
            "isolation",
            "status",
            "completion",
            "quality",
            "quality_mean",
            "quality_lowest",
            "lowest_language",
            "cost_usd",
            "busy_seconds",
            "efficiency",
            "overall",
            "wall_seconds",
            "agents",
            "peak_concurrency",
            "operator_replies",
        ]
        .map(String::from)
        .to_vec();
        for language in Language::ALL {
            columns.push(format!("{}_total", language.as_str()));
        }
        for language in Language::ALL {
            for category in CategoryId::ALL {
                columns.push(format!("{}_{}", language.as_str(), category.as_str()));
            }
        }
        columns
    }

    /// The values of [`Scorecard::tsv_columns`], in the same order. A missing value is an empty cell.
    pub fn tsv_row(&self) -> Vec<String> {
        let headline = &self.headline;
        let mut row = vec![
            cell(&self.run_id),
            cell(&self.label),
            cell(&self.date),
            cell(&self.versions.task),
            cell(&self.versions.rubric),
            cell(&self.versions.judge_model),
            cell(&self.versions.claude_code),
            cell(&self.manifest.claude_code.model),
            json_name(&self.manifest.auth),
            json_name(&self.isolation),
            json_name(&self.validity.status),
            json_name(&self.validity.completion),
            score(headline.quality),
            score(headline.quality_mean),
            score(headline.quality_lowest),
            headline
                .lowest_language
                .map(|l| l.as_str().to_string())
                .unwrap_or_default(),
            format!("{:.2}", headline.cost_usd),
            format!("{:.0}", headline.busy_seconds),
            score(headline.efficiency),
            score(headline.overall),
            format!("{:.0}", self.telemetry.time.wall_seconds),
            self.telemetry.agents.len().to_string(),
            self.telemetry.activity.peak_concurrency.to_string(),
            self.validity.operator_replies.to_string(),
        ];
        for language in Language::ALL {
            row.push(score(self.language(language).map(|l| l.total)));
        }
        for language in Language::ALL {
            for category in CategoryId::ALL {
                let earned = self.language(language).and_then(|l| {
                    l.categories
                        .iter()
                        .find(|c| c.category == category)
                        .map(|c| c.earned)
                });
                row.push(score(earned));
            }
        }
        row
    }

    /// The whole `scorecard.tsv` file: header line, data line, each ending in LF.
    pub fn to_tsv(&self) -> String {
        format!(
            "{}\n{}\n",
            Scorecard::tsv_columns().join("\t"),
            self.tsv_row().join("\t")
        )
    }

    pub fn language(&self, language: Language) -> Option<&LanguageScore> {
        self.languages.iter().find(|l| l.language == language)
    }
}

fn score(value: Option<f64>) -> String {
    value.map(|v| format!("{v:.2}")).unwrap_or_default()
}

fn cell(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

fn json_name<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => name,
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn sample() -> Scorecard {
        let manifest_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("harnesses/claude-code.toml");
        let manifest = HarnessManifest::load(&manifest_path).expect("control manifest is valid");
        let rust = LanguageScore {
            language: Language::Rust,
            build: BuildOutcome::Built,
            golden_pass_rate: 0.97,
            correctness_factor: 1.0,
            total: 88.5,
            categories: CategoryId::ALL
                .iter()
                .map(|&category| CategoryPoints {
                    category,
                    earned: 5.0,
                    max: 10.0,
                })
                .collect(),
            metrics: vec![MetricScore {
                key: "D.mutation".to_string(),
                raw: Some(0.84),
                points: 5.0,
                max: 5.0,
            }],
            groups: vec![GroupRate {
                group: "limit-orders".to_string(),
                passed: 19,
                total: 20,
            }],
            comment_density: Some(0.08),
        };
        Scorecard {
            schema: SCORECARD_SCHEMA,
            run_id: "r7k2q9".to_string(),
            label: manifest.label.clone(),
            date: "2026-11-18T10:00:00Z".to_string(),
            versions: Versions {
                task: "0.1.0".to_string(),
                rubric: "0.1.0".to_string(),
                judge_model: "claude-opus-5-5".to_string(),
                claude_code: "2.1.327".to_string(),
                benchy: "0.1.0".to_string(),
                scorer_image: None,
                workspace: "4f2a".to_string(),
            },
            isolation: IsolationLevel::CleanConfig,
            validity: Validity {
                status: RunStatus::Valid,
                completion: Completion::Finished,
                audit_flags: 0,
                operator_replies: 0,
                uncommitted_work: false,
                notes: vec!["tab\there".to_string()],
            },
            headline: Headline {
                quality: Some(81.25),
                cost_usd: 42.5,
                busy_seconds: 3600.0,
                ..Headline::default()
            },
            languages: vec![rust],
            telemetry: Telemetry::default(),
            manifest,
        }
    }

    #[test]
    fn scorecard_round_trips_through_json() {
        let card = sample();
        let json = serde_json::to_string_pretty(&card).expect("a scorecard serialises");
        let back: Scorecard = serde_json::from_str(&json).expect("it parses again");
        assert_eq!(back, card);
    }

    #[test]
    fn tsv_row_matches_the_fixed_columns() {
        let card = sample();
        let columns = Scorecard::tsv_columns();
        let row = card.tsv_row();
        assert_eq!(columns.len(), 24 + 3 + 27);
        assert_eq!(row.len(), columns.len());
        let value =
            |name: &str| row[columns.iter().position(|c| c == name).expect("column")].clone();
        assert_eq!(value("quality"), "81.25");
        assert_eq!(value("auth"), "api-key");
        assert_eq!(value("rust_total"), "88.50");
        assert_eq!(value("rust_D"), "5.00");
        assert_eq!(value("go_total"), "");
        assert_eq!(value("efficiency"), "");
    }

    #[test]
    fn tsv_file_is_two_lines_with_no_stray_tabs() {
        let tsv = sample().to_tsv();
        let lines: Vec<&str> = tsv.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].split('\t').count(), lines[1].split('\t').count());
        assert!(lines[0].starts_with("run_id\tlabel\tdate\t"));
    }
}
