//! The rubric: every weight, threshold and par band, read from `validation/rubric.toml`.
//! It also fixes how a raw measurement turns into points, so every scorer reads the numbers the same way.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::points::CategoryId;

/// Points of one language score.
pub const LANGUAGE_POINTS: f64 = 100.0;

const TOLERANCE: f64 = 1e-9;

#[derive(Debug, thiserror::Error)]
pub enum RubricError {
    #[error("failed to read rubric {path}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse rubric")]
    Parse(#[from] toml::de::Error),
    #[error("category points add up to {total}, expected {LANGUAGE_POINTS}")]
    TotalPoints { total: f64 },
    #[error("category {category} has {points} points but its metrics add up to {total}")]
    CategoryPoints {
        category: &'static str,
        points: f64,
        total: f64,
    },
    #[error("categories must be A to I in order, found {found:?}")]
    CategoryOrder { found: Vec<CategoryId> },
    #[error("metric {key} has the same value {value} for full and zero")]
    FlatMetric { key: String, value: f64 },
    #[error("metric {key} uses a log scale, so full and zero must be above 0")]
    LogScaleNotPositive { key: String },
    #[error("comment density cap names metric {key}, which the rubric does not define")]
    UnknownCappedMetric { key: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rubric {
    pub version: String,
    pub status: RubricStatus,
    pub quality: QualityWeights,
    pub overall: OverallWeights,
    pub efficiency: Efficiency,
    pub correctness_scaling: CorrectnessScaling,
    pub build_gate: BuildGate,
    pub comment_density: CommentDensityCap,
    pub caps: RunCaps,
    pub judge: JudgeSettings,
    pub measurement: Measurement,
    #[serde(rename = "category")]
    pub categories: Vec<Category>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RubricStatus {
    /// Numbers may still change in the pilot; scorecards of different provisional versions do not compare.
    Provisional,
    Frozen,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualityWeights {
    pub mean_weight: f64,
    pub lowest_weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverallWeights {
    pub base: f64,
    pub efficiency_weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Efficiency {
    pub cost_weight: f64,
    pub time_weight: f64,
    pub best_factor: f64,
    pub worst_factor: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_cost_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_busy_seconds: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorrectnessScaling {
    pub floor: f64,
    pub span: f64,
    pub categories: Vec<CategoryId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildGate {
    pub categories: Vec<CategoryId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommentDensityCap {
    pub density: f64,
    pub factor: f64,
    /// Metric key in the form `G.comment_quality`.
    pub metric: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunCaps {
    pub cost_usd: f64,
    pub wall_hours: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JudgeSettings {
    pub model: String,
    pub samples: u32,
    pub scale_max: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Measurement {
    pub fuzz_streams: u32,
    pub mutants_per_language: u32,
    pub test_runs: u32,
    pub performance_runs: u32,
    pub min_workload_seconds: f64,
    pub max_cyclomatic: u32,
    pub max_function_lines: u32,
    pub max_nesting: u32,
    pub memory_limit_mb: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Category {
    pub id: CategoryId,
    pub name: String,
    pub points: f64,
    #[serde(rename = "metric")]
    pub metrics: Vec<Metric>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metric {
    pub id: String,
    pub name: String,
    pub points: f64,
    pub source: Source,
    pub measure: String,
    pub scale: Scale,
    /// Raw value at or beyond which the metric earns all its points.
    pub full: f64,
    /// Raw value at or beyond which the metric earns nothing.
    pub zero: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Measured,
    Judged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scale {
    Linear,
    Log,
}

impl Rubric {
    /// Reads and checks a rubric file.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be read, does not parse, or breaks a rule of [`Rubric::from_toml`].
    pub fn load(path: &Path) -> Result<Rubric, RubricError> {
        let text = std::fs::read_to_string(path).map_err(|source| RubricError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Rubric::from_toml(&text)
    }

    /// Parses a rubric and checks that its points add up and every metric can be scored.
    ///
    /// # Errors
    ///
    /// Fails on bad TOML, categories out of order, points that do not add up to 100,
    /// a metric with no slope, or a log-scale metric with a bound at or below 0.
    pub fn from_toml(text: &str) -> Result<Rubric, RubricError> {
        let rubric: Rubric = toml::from_str(text)?;
        rubric.check()?;
        Ok(rubric)
    }

    fn check(&self) -> Result<(), RubricError> {
        let found: Vec<CategoryId> = self.categories.iter().map(|c| c.id).collect();
        if found != CategoryId::ALL {
            return Err(RubricError::CategoryOrder { found });
        }
        let total: f64 = self.categories.iter().map(|c| c.points).sum();
        if (total - LANGUAGE_POINTS).abs() > TOLERANCE {
            return Err(RubricError::TotalPoints { total });
        }
        for category in &self.categories {
            category.check()?;
        }
        if self.metric(&self.comment_density.metric).is_none() {
            return Err(RubricError::UnknownCappedMetric {
                key: self.comment_density.metric.clone(),
            });
        }
        Ok(())
    }

    pub fn category(&self, id: CategoryId) -> &Category {
        self.categories
            .iter()
            .find(|c| c.id == id)
            .expect("a checked rubric holds every category")
    }

    /// Finds a metric by its key, such as `D.mutation`.
    pub fn metric(&self, key: &str) -> Option<&Metric> {
        let (category, id) = key.split_once('.')?;
        let category = self.categories.iter().find(|c| c.id.as_str() == category)?;
        category.metrics.iter().find(|m| m.id == id)
    }

    /// Total points of every metric with this source, over one language score.
    pub fn points_from(&self, source: Source) -> f64 {
        self.categories
            .iter()
            .flat_map(|c| &c.metrics)
            .filter(|m| m.source == source)
            .map(|m| m.points)
            .sum()
    }

    /// The factor that categories under correctness scaling are multiplied by, for golden pass rate `pass_rate`.
    pub fn correctness_factor(&self, pass_rate: f64) -> f64 {
        let scaling = &self.correctness_scaling;
        let factor = (pass_rate - scaling.floor) / scaling.span;
        // A pass rate of exactly floor + span must give full credit, which plain division misses by rounding.
        if factor > 1.0 - TOLERANCE {
            return 1.0;
        }
        factor.max(0.0)
    }

    /// Quality from the three language scores.
    pub fn quality(&self, language_scores: [f64; 3]) -> f64 {
        let mean = language_scores.iter().sum::<f64>() / 3.0;
        let lowest = language_scores
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        self.quality.mean_weight * mean + self.quality.lowest_weight * lowest
    }

    /// Overall from Quality and Efficiency, both on a 0 to 100 scale.
    pub fn overall(&self, quality: f64, efficiency: f64) -> f64 {
        quality * (self.overall.base + self.overall.efficiency_weight * efficiency / 100.0)
    }

    /// Efficiency on a 0 to 100 scale, or `None` while the pilot has not set the baselines.
    pub fn efficiency(&self, cost_usd: f64, busy_seconds: f64) -> Option<f64> {
        let efficiency = &self.efficiency;
        let cost = self.efficiency_part(cost_usd, efficiency.baseline_cost_usd?);
        let time = self.efficiency_part(busy_seconds, efficiency.baseline_busy_seconds?);
        Some(100.0 * (efficiency.cost_weight * cost + efficiency.time_weight * time))
    }

    fn efficiency_part(&self, value: f64, baseline: f64) -> f64 {
        let best = self.efficiency.best_factor * baseline;
        let worst = self.efficiency.worst_factor * baseline;
        Scale::Log.fraction(value, best, worst)
    }
}

impl Category {
    fn check(&self) -> Result<(), RubricError> {
        let total: f64 = self.metrics.iter().map(|m| m.points).sum();
        if (total - self.points).abs() > TOLERANCE {
            return Err(RubricError::CategoryPoints {
                category: self.id.as_str(),
                points: self.points,
                total,
            });
        }
        for metric in &self.metrics {
            let key = format!("{}.{}", self.id.as_str(), metric.id);
            if (metric.full - metric.zero).abs() < TOLERANCE {
                return Err(RubricError::FlatMetric {
                    key,
                    value: metric.full,
                });
            }
            if metric.scale == Scale::Log && (metric.full <= 0.0 || metric.zero <= 0.0) {
                return Err(RubricError::LogScaleNotPositive { key });
            }
        }
        Ok(())
    }
}

impl Metric {
    /// Points for a raw measurement: all of them at `full`, none at `zero`, and in between along the scale.
    pub fn points_for(&self, raw: f64) -> f64 {
        self.points * self.scale.fraction(raw, self.full, self.zero)
    }
}

impl Scale {
    /// Where `raw` lies between `zero` (0.0) and `full` (1.0), clamped to that range.
    /// Either bound may be the larger, so the same rule serves metrics where less is better.
    fn fraction(self, raw: f64, full: f64, zero: f64) -> f64 {
        let raw = raw.clamp(full.min(zero), full.max(zero));
        let fraction = match self {
            Scale::Linear => (raw - zero) / (full - zero),
            Scale::Log => (raw.ln() - zero.ln()) / (full.ln() - zero.ln()),
        };
        fraction.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn repo_rubric() -> Rubric {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("validation/rubric.toml");
        Rubric::load(&path).expect("validation/rubric.toml is a valid rubric")
    }

    #[test]
    fn repo_rubric_round_trips_through_toml() {
        let rubric = repo_rubric();
        let text = toml::to_string(&rubric).expect("a rubric serialises");
        assert_eq!(Rubric::from_toml(&text).expect("it parses again"), rubric);
    }

    #[test]
    fn measured_and_judged_points_split_88_to_12() {
        let rubric = repo_rubric();
        assert!((rubric.points_from(Source::Measured) - 88.0).abs() < TOLERANCE);
        assert!((rubric.points_from(Source::Judged) - 12.0).abs() < TOLERANCE);
    }

    #[test]
    fn worked_checks_of_the_overall_formula_hold() {
        let rubric = repo_rubric();
        assert_eq!(rubric.overall(85.0, 7.0).round(), 73.0);
        assert_eq!(rubric.overall(65.0, 86.0).round(), 64.0);
    }

    #[test]
    fn correctness_factor_needs_95_percent_for_full_credit() {
        let rubric = repo_rubric();
        assert_eq!(rubric.correctness_factor(0.5), 0.0);
        assert_eq!(rubric.correctness_factor(0.95), 1.0);
        assert!((rubric.correctness_factor(0.725) - 0.5).abs() < TOLERANCE);
    }

    #[test]
    fn metric_points_follow_the_scale_in_both_directions() {
        let rubric = repo_rubric();
        let lines = rubric.metric("E.production_lines").expect("defined");
        assert_eq!(lines.points_for(1.0), 3.0);
        assert!((lines.points_for(1.9) - 1.5).abs() < TOLERANCE);
        assert_eq!(lines.points_for(4.0), 0.0);
        let throughput = rubric.metric("C.throughput").expect("defined");
        assert_eq!(throughput.points_for(0.5), 3.0);
        assert_eq!(throughput.points_for(20.0), 0.0);
        let middle = (1.25_f64 * 10.0).sqrt();
        assert!((throughput.points_for(middle) - 1.5).abs() < 1e-6);
    }

    #[test]
    fn efficiency_waits_for_the_pilot_baselines() {
        let mut rubric = repo_rubric();
        assert_eq!(rubric.efficiency(10.0, 600.0), None);
        rubric.efficiency.baseline_cost_usd = Some(40.0);
        rubric.efficiency.baseline_busy_seconds = Some(3600.0);
        assert_eq!(rubric.efficiency(10.0, 900.0), Some(100.0));
        assert_eq!(rubric.efficiency(320.0, 28_800.0), Some(0.0));
    }

    #[test]
    fn points_that_do_not_add_up_are_rejected() {
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("validation/rubric.toml"),
        )
        .expect("rubric is readable");
        let broken = text.replacen("points = 27", "points = 28", 1);
        assert!(matches!(
            Rubric::from_toml(&broken),
            Err(RubricError::CategoryPoints { category: "A", .. })
        ));
    }
}
