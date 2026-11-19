//! Aggregation tests: the formulas of the scoring specification, the build gate, the correctness scaling and validity.

use std::path::Path;

use benchy::contract::{
    BuildOutcome, CategoryId, HarnessManifest, Language, MEASURED_SCHEMA, MeasuredLanguage,
    MeasuredScores, MetricScore, Rubric, RunStatus, Source, Telemetry,
};

use super::{Inputs, Review, ReviewStatus, aggregate};
use crate::judge::{JudgedItem, JudgedScores};
use crate::run::record::{FinishRecord, RunRecord};

fn repo(path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn rubric() -> Rubric {
    Rubric::load(&repo("validation/rubric.toml")).expect("the repo rubric is valid")
}

/// Every measured metric at full points, for one language at golden pass rate `pass_rate`.
fn full_language(rubric: &Rubric, language: Language, pass_rate: f64) -> MeasuredLanguage {
    let metrics = rubric
        .categories
        .iter()
        .flat_map(|c| c.metrics.iter().map(move |m| (c.id, m)))
        .filter(|(_, m)| m.source == Source::Measured)
        .map(|(category, m)| MetricScore {
            key: format!("{}.{}", category.as_str(), m.id),
            raw: Some(m.full),
            points: m.points,
            max: m.points,
        })
        .collect();
    MeasuredLanguage {
        language,
        build: BuildOutcome::Built,
        golden_pass_rate: pass_rate,
        metrics,
        groups: Vec::new(),
        comment_density: None,
        notes: Vec::new(),
    }
}

fn measured(languages: Vec<MeasuredLanguage>) -> MeasuredScores {
    MeasuredScores {
        schema: MEASURED_SCHEMA,
        workspace: "w".to_string(),
        scorer_image: None,
        languages,
    }
}

fn judged_all(rubric: &Rubric, level: u32) -> JudgedScores {
    let mut items = Vec::new();
    for language in Language::ALL {
        for category in &rubric.categories {
            for metric in category
                .metrics
                .iter()
                .filter(|m| m.source == Source::Judged)
            {
                items.push(JudgedItem {
                    language,
                    key: format!("{}.{}", category.id.as_str(), metric.id),
                    levels: vec![level; 3],
                    median: level,
                    spread: 0,
                    evidence: vec!["rust/src/lib.rs:1".to_string()],
                    reason: "fixture".to_string(),
                });
            }
        }
    }
    JudgedScores {
        model: "claude-opus-5-5".to_string(),
        prompt_hash: "h".to_string(),
        samples: 3,
        items,
    }
}

fn record(audit_findings: Option<u32>) -> RunRecord {
    RunRecord {
        id: "abc123".to_string(),
        label: "claude-code".to_string(),
        run_dir: "/r/abc123".into(),
        task_pack: "/task".into(),
        created: "2026-01-01T00:00:00.000Z".to_string(),
        launch: None,
        finish: Some(FinishRecord {
            ended: "2026-01-01T01:00:00.000Z".to_string(),
            head: Some("h".to_string()),
            branch: "main".to_string(),
            uncommitted: Vec::new(),
            audit_findings,
        }),
    }
}

struct Case {
    rubric: Rubric,
    measured: MeasuredScores,
    judged: JudgedScores,
    telemetry: Telemetry,
    manifest: HarnessManifest,
    record: RunRecord,
}

fn case(pass_rates: [f64; 3]) -> Case {
    let rubric = rubric();
    let languages = Language::ALL
        .iter()
        .zip(pass_rates)
        .map(|(&l, p)| full_language(&rubric, l, p))
        .collect();
    Case {
        measured: measured(languages),
        judged: judged_all(&rubric, 4),
        rubric,
        telemetry: Telemetry::default(),
        manifest: HarnessManifest::load(&repo("harnesses/claude-code.toml")).expect("valid"),
        record: record(Some(0)),
    }
}

impl Case {
    fn score(&self, review: Option<&Review>) -> benchy::contract::Scorecard {
        aggregate(&Inputs {
            rubric: &self.rubric,
            measured: &self.measured,
            judged: Some(&self.judged),
            telemetry: &self.telemetry,
            manifest: &self.manifest,
            record: &self.record,
            review,
            task_version: "0.1.0",
        })
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn worked_checks_of_the_overall_formula_hold() {
    let rubric = rubric();
    assert_eq!(rubric.overall(85.0, 7.0).round(), 73.0);
    assert_eq!(rubric.overall(65.0, 86.0).round(), 64.0);
}

#[test]
fn correctness_scaling_multiplies_categories_c_to_i() {
    // A and B (50 points) are never scaled; C to I (50 points) carry the factor.
    let card = case([0.5, 0.725, 0.95]).score(None);
    let totals: Vec<f64> = card.languages.iter().map(|l| l.total).collect();
    let factors: Vec<f64> = card
        .languages
        .iter()
        .map(|l| l.correctness_factor)
        .collect();
    assert!(
        close(factors[0], 0.0) && close(factors[1], 0.5) && close(factors[2], 1.0),
        "{factors:?}"
    );
    assert!(
        close(totals[0], 50.0) && close(totals[1], 75.0) && close(totals[2], 100.0),
        "{totals:?}"
    );
}

#[test]
fn quality_weighs_the_mean_and_the_lowest_language() {
    let card = case([0.5, 0.725, 0.95]).score(None);
    let headline = &card.headline;
    assert!(close(headline.quality_mean.expect("set"), 75.0));
    assert!(close(headline.quality_lowest.expect("set"), 50.0));
    assert!(close(
        headline.quality.expect("set"),
        0.6 * 75.0 + 0.4 * 50.0
    ));
    assert_eq!(headline.lowest_language, Some(Language::Rust));
    assert_eq!(
        headline.efficiency, None,
        "the rubric has no pilot baseline yet"
    );
    assert_eq!(headline.overall, None);
}

#[test]
fn efficiency_and_overall_follow_the_baselines_once_set() {
    let mut case = case([1.0, 1.0, 1.0]);
    case.rubric.efficiency.baseline_cost_usd = Some(40.0);
    case.rubric.efficiency.baseline_busy_seconds = Some(3600.0);
    case.telemetry.cost_usd = 10.0;
    case.telemetry.time.busy_seconds = 900.0;
    let headline = case.score(None).headline;
    assert_eq!(headline.efficiency, Some(100.0));
    assert!(close(headline.overall.expect("set"), 100.0));
}

#[test]
fn a_language_that_does_not_build_loses_categories_a_to_d() {
    let mut case = case([1.0, 1.0, 1.0]);
    case.measured.languages[1].build = BuildOutcome::Failed;
    let card = case.score(None);
    let go = card.language(Language::Go).expect("go is scored");
    for category in &go.categories {
        let gated = matches!(
            category.category,
            CategoryId::A | CategoryId::B | CategoryId::C | CategoryId::D
        );
        assert_eq!(category.earned == 0.0, gated, "{category:?}");
    }
    assert!(close(go.total, 100.0 - 45.0 - 5.0 - 8.0 - 12.0));
}

#[test]
fn a_category_with_no_metric_scores_zero_and_is_reported_as_not_measured() {
    let mut case = case([1.0, 1.0, 1.0]);
    for language in &mut case.measured.languages {
        language.metrics.retain(|m| !m.key.starts_with("D."));
    }
    let card = case.score(None);
    let rust = card.language(Language::Rust).expect("rust is scored");
    assert!(close(rust.total, 88.0));
    assert!(
        card.validity.notes.iter().any(
            |n| n == "rust: category D (Tests the harness wrote) was not measured and scores 0"
        ),
        "{:?}",
        card.validity.notes
    );
}

#[test]
fn audit_findings_need_review_and_a_void_run_has_no_headline() {
    let mut case = case([1.0, 1.0, 1.0]);
    case.record = record(Some(2));
    assert_eq!(case.score(None).validity.status, RunStatus::NeedsReview);
    let void = Review {
        status: ReviewStatus::Void,
        note: "read the rubric".to_string(),
    };
    let card = case.score(Some(&void));
    assert_eq!(card.validity.status, RunStatus::Void);
    assert_eq!(card.headline.quality, None);
    assert_eq!(card.headline.overall, None);
}

#[test]
fn judged_levels_earn_their_share_of_the_points() {
    let mut case = case([1.0, 1.0, 1.0]);
    case.judged = judged_all(&case.rubric, 2);
    let card = case.score(None);
    let rust = card.language(Language::Rust).expect("scored");
    let judged = case.rubric.points_from(Source::Judged);
    assert!(close(rust.total, 100.0 - judged / 2.0));
}

#[test]
fn comment_density_above_the_cap_halves_the_comment_quality_points() {
    let mut case = case([1.0, 1.0, 1.0]);
    let cap = case.rubric.comment_density.clone();
    case.measured.languages[0].comment_density = Some(cap.density + 0.05);
    case.measured.languages[1].comment_density = Some(cap.density);
    let card = case.score(None);
    let points = |language: usize| {
        card.languages[language]
            .metrics
            .iter()
            .find(|m| m.key == cap.metric)
            .map(|m| (m.points, m.max))
            .expect("the capped metric is scored")
    };
    let (dense, max) = points(0);
    assert!(close(dense, max * cap.factor), "{dense} of {max}");
    let (at_the_limit, max) = points(1);
    assert!(
        close(at_the_limit, max),
        "density at the limit is not capped"
    );
    assert!(
        card.validity
            .notes
            .iter()
            .any(|n| n.contains("comment density") && n.starts_with("rust")),
        "{:?}",
        card.validity.notes
    );
}
