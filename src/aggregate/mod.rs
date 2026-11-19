//! Aggregation: the one place that applies the build gate and the correctness scaling and computes every total.
//! It turns measured scores, judged scores and telemetry into the scorecard of one run.

use benchy::contract::{
    BuildOutcome, CategoryId, CategoryPoints, Completion, HarnessManifest, Headline, Language,
    LanguageScore, MeasuredLanguage, MeasuredScores, MetricScore, Rubric, RunStatus,
    SCORECARD_SCHEMA, Scorecard, Source, Telemetry, Validity, Versions,
};
use serde::Deserialize;

use crate::judge::JudgedScores;
use crate::run::record::RunRecord;

/// The operator's decision on a run whose audit found something, from `.runs/<run-id>/review.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub status: ReviewStatus,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewStatus {
    /// The findings were reviewed and no benchy material was read.
    Valid,
    /// A confirmed read of benchy material.
    Void,
}

/// Everything one scorecard is made from.
#[derive(Debug, Clone)]
pub struct Inputs<'a> {
    pub rubric: &'a Rubric,
    pub measured: &'a MeasuredScores,
    pub judged: Option<&'a JudgedScores>,
    pub telemetry: &'a Telemetry,
    pub manifest: &'a HarnessManifest,
    pub record: &'a RunRecord,
    pub review: Option<&'a Review>,
    pub task_version: &'a str,
}

pub fn aggregate(inputs: &Inputs) -> Scorecard {
    let mut notes = Vec::new();
    let languages: Vec<LanguageScore> = Language::ALL
        .iter()
        .map(|&language| score_language(inputs, language, &mut notes))
        .collect();
    let validity = validity(inputs, notes);
    let headline = headline(inputs, &languages, validity.status, validity.completion);
    let measured = inputs.measured;
    Scorecard {
        schema: SCORECARD_SCHEMA,
        run_id: inputs.record.id.clone(),
        label: inputs.record.label.clone(),
        date: inputs
            .record
            .launch
            .as_ref()
            .map_or_else(|| inputs.record.created.clone(), |l| l.started.clone()),
        versions: Versions {
            task: inputs.task_version.to_string(),
            rubric: inputs.rubric.version.clone(),
            judge_model: inputs.rubric.judge.model.clone(),
            claude_code: claude_code_version(inputs),
            benchy: env!("CARGO_PKG_VERSION").to_string(),
            scorer_image: measured.scorer_image.clone(),
            workspace: measured.workspace.clone(),
        },
        isolation: inputs.manifest.isolation,
        validity,
        headline,
        languages,
        telemetry: inputs.telemetry.clone(),
        manifest: inputs.manifest.clone(),
    }
}

fn claude_code_version(inputs: &Inputs) -> String {
    let seen = &inputs.telemetry.claude_code_versions;
    if seen.is_empty() {
        inputs.manifest.claude_code.version.clone()
    } else {
        seen.join(",")
    }
}

/// A language missing from `metrics.json` is scored as if nothing of it was measured.
fn score_language(inputs: &Inputs, language: Language, notes: &mut Vec<String>) -> LanguageScore {
    let rubric = inputs.rubric;
    let fallback = MeasuredLanguage {
        language,
        build: BuildOutcome::Failed,
        golden_pass_rate: 0.0,
        metrics: Vec::new(),
        groups: Vec::new(),
        comment_density: None,
        notes: vec!["not in metrics.json".to_string()],
    };
    let measured = inputs
        .measured
        .languages
        .iter()
        .find(|l| l.language == language)
        .unwrap_or(&fallback);
    for note in &measured.notes {
        notes.push(format!("{}: {note}", language.as_str()));
    }
    let factor = rubric.correctness_factor(measured.golden_pass_rate);
    let mut categories = Vec::new();
    let mut metrics = Vec::new();
    for category in &rubric.categories {
        let gated = measured.build == BuildOutcome::Failed
            && rubric.build_gate.categories.contains(&category.id);
        let scale = if gated {
            0.0
        } else if rubric.correctness_scaling.categories.contains(&category.id) {
            factor
        } else {
            1.0
        };
        let mut any_scored = false;
        let mut earned = 0.0;
        for metric in &category.metrics {
            let key = format!("{}.{}", category.id.as_str(), metric.id);
            let (raw, unscaled) = match metric.source {
                Source::Measured => measured
                    .metrics
                    .iter()
                    .find(|m| m.key == key)
                    .map(|m| (m.raw, m.points.clamp(0.0, metric.points))),
                Source::Judged => inputs
                    .judged
                    .and_then(|j| j.item(language, &key))
                    .map(|item| {
                        let level = f64::from(item.median);
                        (Some(level), metric.points_for(level))
                    }),
            }
            .map_or((None, 0.0), |(raw, points)| {
                any_scored = true;
                (raw, points)
            });
            let unscaled = match comment_cap(rubric, measured, &key, metric.points) {
                Some(cap) if unscaled > cap => {
                    notes.push(format!(
                        "{}: comment density {:.0}% is above {:.0}%, so {key} is capped at {cap:.2}",
                        language.as_str(),
                        measured.comment_density.unwrap_or_default() * 100.0,
                        rubric.comment_density.density * 100.0
                    ));
                    cap
                }
                _ => unscaled,
            };
            let points = unscaled * scale;
            earned += points;
            metrics.push(MetricScore {
                key,
                raw,
                points,
                max: metric.points,
            });
        }
        if !any_scored {
            notes.push(format!(
                "{}: category {} ({}) was not measured and scores 0",
                language.as_str(),
                category.id.as_str(),
                category.name
            ));
        }
        categories.push(CategoryPoints {
            category: category.id,
            earned,
            max: category.points,
        });
    }
    if measured.build == BuildOutcome::Failed {
        notes.push(format!(
            "{}: did not build, so categories {} score 0",
            language.as_str(),
            names(&rubric.build_gate.categories)
        ));
    }
    LanguageScore {
        language,
        build: measured.build,
        golden_pass_rate: measured.golden_pass_rate,
        correctness_factor: factor,
        total: categories.iter().map(|c| c.earned).sum(),
        categories,
        metrics,
        groups: measured.groups.clone(),
        comment_density: measured.comment_density,
    }
}

/// The most that `key` can earn when the comment-density cap of the rubric applies to it.
fn comment_cap(rubric: &Rubric, measured: &MeasuredLanguage, key: &str, max: f64) -> Option<f64> {
    let cap = &rubric.comment_density;
    let dense = measured.comment_density.is_some_and(|d| d > cap.density);
    (dense && key == cap.metric).then_some(max * cap.factor)
}

fn names(categories: &[CategoryId]) -> String {
    categories
        .iter()
        .map(|c| c.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn validity(inputs: &Inputs, mut notes: Vec<String>) -> Validity {
    let finish = inputs.record.finish.as_ref();
    let audit_flags = finish.and_then(|f| f.audit_findings);
    if finish.is_some() && audit_flags.is_none() {
        notes.push("the audit could not read the transcripts".to_string());
    }
    let audit_flags = audit_flags.unwrap_or(0);
    let status = match inputs.review.map(|r| r.status) {
        Some(ReviewStatus::Void) => RunStatus::Void,
        Some(ReviewStatus::Valid) => RunStatus::Valid,
        None if audit_flags > 0 => RunStatus::NeedsReview,
        None => RunStatus::Valid,
    };
    if let Some(review) = inputs.review {
        notes.push(format!("operator review: {}", review.note));
    }
    let launch = inputs.record.launch.as_ref();
    let completion = match launch {
        Some(l) if l.hit_time_limit => Completion::TimeCap,
        Some(l) if l.hit_a_cap() => Completion::CostCap,
        _ => Completion::Finished,
    };
    if !inputs.telemetry.manual_entries.is_empty() {
        notes.push(format!(
            "entered by hand: {}",
            inputs.telemetry.manual_entries.join(", ")
        ));
    }
    if inputs.judged.is_none() {
        notes.push("not judged: every judged item scores 0".to_string());
    }
    Validity {
        status,
        completion,
        audit_flags,
        operator_replies: inputs.telemetry.activity.operator_replies,
        uncommitted_work: finish.is_some_and(|f| !f.uncommitted.is_empty()),
        notes,
    }
}

fn headline(
    inputs: &Inputs,
    languages: &[LanguageScore],
    status: RunStatus,
    completion: Completion,
) -> Headline {
    let rubric = inputs.rubric;
    let cost_usd = inputs.telemetry.total_cost_usd();
    let busy_seconds = inputs.telemetry.time.busy_seconds;
    if status == RunStatus::Void {
        return Headline {
            cost_usd,
            busy_seconds,
            ..Headline::default()
        };
    }
    let totals = [languages[0].total, languages[1].total, languages[2].total];
    let lowest = languages
        .iter()
        .min_by(|a, b| a.total.total_cmp(&b.total))
        .expect("there are three languages");
    let quality = rubric.quality(totals);
    // A run stopped by a cap has Efficiency scored at that cap.
    let (scored_cost, scored_busy) = match completion {
        Completion::CostCap => (cost_usd.max(rubric.caps.cost_usd), busy_seconds),
        Completion::TimeCap => (cost_usd, busy_seconds.max(rubric.caps.wall_hours * 3600.0)),
        Completion::Finished => (cost_usd, busy_seconds),
    };
    let efficiency = rubric.efficiency(scored_cost, scored_busy);
    Headline {
        quality: Some(quality),
        quality_mean: Some(totals.iter().sum::<f64>() / 3.0),
        quality_lowest: Some(lowest.total),
        lowest_language: Some(lowest.language),
        cost_usd,
        busy_seconds,
        efficiency,
        overall: efficiency.map(|e| rubric.overall(quality, e)),
    }
}

#[cfg(test)]
mod tests;
