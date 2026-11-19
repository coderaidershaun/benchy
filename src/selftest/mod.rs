//! `benchy selftest`: scores synthetic workspaces made from the references and checks that the rubric orders them as it must.
//! It makes no paid call: judged items come from stored levels in `validation/selftest/`, through the same aggregation a run uses.

mod variants;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Deserialize;

use crate::aggregate::{Inputs, aggregate};
use crate::judge::{JudgedItem, JudgedScores};
use crate::run::record::RunRecord;
use crate::score::{self, ScoreError};
use benchy::contract::{
    CategoryId, HarnessManifest, Language, LanguageScore, MeasuredScores, Rubric, Source, Telemetry,
};
use variants::Breakage;

#[derive(Debug, clap::Args)]
pub struct SelftestArgs {
    /// Score on this machine's toolchains instead of inside the scorer image.
    #[arg(long)]
    pub host: bool,
    /// Folder for the synthetic workspaces and their scores. Kept when given; a fresh temporary folder otherwise.
    #[arg(long)]
    pub scratch: Option<PathBuf>,
    /// Use this metrics.json of the reference workspace instead of scoring it again, which takes the longest.
    #[arg(long)]
    pub reference_metrics: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum SelftestError {
    #[error("failed to {action}")]
    Io {
        action: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to score the {variant} workspace")]
    Score {
        variant: &'static str,
        #[source]
        source: ScoreError,
    },
    #[error("failed to read {path}: {reason}")]
    Parse { path: PathBuf, reason: String },
    #[error("{0}")]
    Setup(String),
}

fn io(action: String) -> impl FnOnce(std::io::Error) -> SelftestError {
    move |source| SelftestError::Io { action, source }
}

/// The synthetic workspaces, each the reference with one change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Variant {
    Reference,
    NoTests,
    DeadCode,
    TautologicalTests,
    BrokenFeature,
    FillerDocs,
}

const VARIANTS: [Variant; 6] = [
    Variant::Reference,
    Variant::NoTests,
    Variant::DeadCode,
    Variant::TautologicalTests,
    Variant::BrokenFeature,
    Variant::FillerDocs,
];

use CategoryId::{A, B, C, D, E, F, H, I};

impl Variant {
    fn name(self) -> &'static str {
        match self {
            Variant::Reference => "reference",
            Variant::NoTests => "tests-deleted",
            Variant::DeadCode => "dead-code",
            Variant::TautologicalTests => "tautological-tests",
            Variant::BrokenFeature => "broken-feature",
            Variant::FillerDocs => "filler-docs",
        }
    }

    /// The measured categories this workspace is scored in. The others are the reference's, and why is in [`Variant::why_copied`].
    fn measured(self) -> &'static [CategoryId] {
        match self {
            Variant::Reference => &[A, B, C, D, E, F, H, I],
            Variant::NoTests | Variant::DeadCode => &[D, E, F, H, I],
            Variant::TautologicalTests => &[D],
            Variant::BrokenFeature => &[A, B, D, E, F, H, I],
            Variant::FillerDocs => &[E, F, H, I],
        }
    }

    fn why_copied(self) -> &'static str {
        match self {
            Variant::Reference => "",
            Variant::NoTests => "A, B and C run the program, which has no test code in it",
            Variant::DeadCode => "A, B and C run the program, and the added functions never run",
            Variant::TautologicalTests => "only the mutation score is compared",
            Variant::BrokenFeature => {
                "C is the reference's, more than a wrong engine can earn, so the total is an upper bound"
            }
            Variant::FillerDocs => {
                "A, B and C run the program and D runs the tests, and comments change neither the code nor its mutants"
            }
        }
    }

    fn apply(self, root: &Path, breakage: &Breakage) -> std::io::Result<()> {
        match self {
            Variant::Reference => Ok(()),
            Variant::NoTests => variants::delete_tests(root),
            Variant::DeadCode => variants::pad_with_dead_code(root),
            Variant::TautologicalTests => variants::make_tests_tautological(root),
            Variant::BrokenFeature => variants::break_feature(root, breakage),
            Variant::FillerDocs => variants::add_filler_docs(root),
        }
    }
}

/// The judged levels the self-test uses in place of calling the judge.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredLevels {
    levels: BTreeMap<String, u32>,
}

fn read_toml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, SelftestError> {
    let text = fs::read_to_string(path).map_err(io(format!("read {}", path.display())))?;
    toml::from_str(&text).map_err(|error| SelftestError::Parse {
        path: path.to_path_buf(),
        reason: error.to_string(),
    })
}

fn judged_scores(rubric: &Rubric, stored: &StoredLevels) -> Result<JudgedScores, SelftestError> {
    let mut items = Vec::new();
    for category in &rubric.categories {
        for metric in category
            .metrics
            .iter()
            .filter(|m| m.source == Source::Judged)
        {
            let key = format!("{}.{}", category.id.as_str(), metric.id);
            let level = *stored.levels.get(&key).ok_or_else(|| {
                SelftestError::Setup(format!(
                    "validation/selftest/judged.toml has no level for {key}"
                ))
            })?;
            for language in Language::ALL {
                items.push(JudgedItem {
                    language,
                    key: key.clone(),
                    levels: vec![level],
                    median: level,
                    spread: 0,
                    evidence: Vec::new(),
                    reason: "stored level of the self-test".to_string(),
                });
            }
        }
    }
    Ok(JudgedScores {
        model: "stored".to_string(),
        prompt_hash: "selftest".to_string(),
        samples: 1,
        items,
    })
}

/// Everything aggregation needs besides the scores, for a workspace that no harness made.
struct Fixed {
    rubric: Rubric,
    judged: JudgedScores,
    manifest: HarnessManifest,
    telemetry: Telemetry,
    task_version: String,
}

impl Fixed {
    fn language_scores(&self, variant: Variant, measured: &MeasuredScores) -> Vec<LanguageScore> {
        let record = RunRecord {
            id: format!("selftest-{}", variant.name()),
            label: "selftest".to_string(),
            run_dir: PathBuf::new(),
            task_pack: PathBuf::new(),
            created: String::new(),
            launch: None,
            finish: None,
        };
        aggregate(&Inputs {
            rubric: &self.rubric,
            measured,
            judged: Some(&self.judged),
            telemetry: &self.telemetry,
            manifest: &self.manifest,
            record: &record,
            review: None,
            task_version: &self.task_version,
        })
        .languages
    }
}

/// Runs `benchy selftest` and returns whether every ordering holds.
///
/// # Errors
///
/// Fails when a workspace cannot be made or scored, or a stored file cannot be read.
pub fn run(args: &SelftestArgs) -> Result<bool, SelftestError> {
    let started = Instant::now();
    let benchy = crate::run::layout::benchy_root()
        .map_err(|error| SelftestError::Setup(error.to_string()))?;
    let stored = benchy.join("validation/selftest");
    let rubric = Rubric::load(&benchy.join("validation/rubric.toml"))
        .map_err(|error| SelftestError::Setup(error.to_string()))?;
    let levels: StoredLevels = read_toml(&stored.join("judged.toml"))?;
    let breakage: Breakage = read_toml(&stored.join("broken.toml"))?;
    let fixed = Fixed {
        judged: judged_scores(&rubric, &levels)?,
        rubric,
        manifest: HarnessManifest::load(&benchy.join("harnesses/claude-code.toml"))
            .map_err(|error| SelftestError::Setup(error.to_string()))?,
        telemetry: Telemetry::default(),
        task_version: fs::read_to_string(benchy.join("task/VERSION"))
            .unwrap_or_default()
            .trim()
            .to_string(),
    };
    let (scratch, owned) = match &args.scratch {
        Some(dir) => (dir.clone(), false),
        None => (
            std::env::temp_dir().join(format!("benchy-selftest-{}", std::process::id())),
            true,
        ),
    };
    let result = score_variants(&benchy, &scratch, args, &breakage, &fixed);
    if owned {
        drop(fs::remove_dir_all(&scratch));
    }
    let scores = result?;
    let checks = orderings(&scores, &breakage.feature);
    for check in &checks {
        println!(
            "{} {}: {}",
            if check.holds { "ok    " } else { "FAILED" },
            check.name,
            check.numbers
        );
    }
    println!(
        "selftest took {:.0} minutes",
        started.elapsed().as_secs_f64() / 60.0
    );
    Ok(checks.iter().all(|check| check.holds))
}

/// The language scores of each variant, in the order of [`VARIANTS`].
type Scores = Vec<(Variant, Vec<LanguageScore>)>;

fn score_variants(
    benchy: &Path,
    scratch: &Path,
    args: &SelftestArgs,
    breakage: &Breakage,
    fixed: &Fixed,
) -> Result<Scores, SelftestError> {
    let mut reference: Option<MeasuredScores> = match &args.reference_metrics {
        Some(path) => {
            let text = fs::read_to_string(path).map_err(io(format!("read {}", path.display())))?;
            Some(
                serde_json::from_str(&text).map_err(|error| SelftestError::Parse {
                    path: path.clone(),
                    reason: error.to_string(),
                })?,
            )
        }
        None => None,
    };
    let mut scores = Vec::new();
    for variant in VARIANTS {
        let step = Instant::now();
        let dir = scratch.join(variant.name());
        let measured = match (&reference, variant) {
            (Some(reference), Variant::Reference) => {
                eprintln!("selftest: the reference scores come from {}", display(args));
                reference.clone()
            }
            _ => {
                let workspace = dir.join("workspace");
                make_workspace(benchy, &workspace, variant, breakage)?;
                eprintln!(
                    "selftest: scoring {} in categories {}",
                    variant.name(),
                    names(variant.measured())
                );
                score::score_workspace(
                    benchy,
                    &workspace,
                    &dir.join("metrics.json"),
                    variant.measured(),
                    args.host,
                )
                .map_err(|source| SelftestError::Score {
                    variant: variant.name(),
                    source,
                })?
            }
        };
        let measured = match &reference {
            Some(reference) => with_reference_categories(measured, reference, variant),
            None => measured,
        };
        if variant == Variant::Reference {
            reference = Some(measured.clone());
        } else {
            eprintln!("selftest: {}: {}", variant.name(), variant.why_copied());
        }
        eprintln!(
            "selftest: {} took {:.1} minutes",
            variant.name(),
            step.elapsed().as_secs_f64() / 60.0
        );
        scores.push((variant, fixed.language_scores(variant, &measured)));
    }
    Ok(scores)
}

fn display(args: &SelftestArgs) -> String {
    args.reference_metrics
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default()
}

fn names(categories: &[CategoryId]) -> String {
    categories
        .iter()
        .map(|c| c.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

fn make_workspace(
    benchy: &Path,
    workspace: &Path,
    variant: Variant,
    breakage: &Breakage,
) -> Result<(), SelftestError> {
    let setup = |error: ScoreError| SelftestError::Setup(error.to_string());
    score::workspace::assemble_reference(benchy, workspace).map_err(setup)?;
    variant
        .apply(workspace, breakage)
        .map_err(io(format!("make the {} workspace", variant.name())))?;
    if variant != Variant::Reference {
        score::workspace::git(workspace, &["add", "--all"]).map_err(setup)?;
        score::workspace::git(workspace, &["commit", "--quiet", "-m", variant.name()])
            .map_err(setup)?;
    }
    Ok(())
}

/// The variant's scores with the metrics of every category it was not scored in taken from the reference.
fn with_reference_categories(
    mut measured: MeasuredScores,
    reference: &MeasuredScores,
    variant: Variant,
) -> MeasuredScores {
    let kept = variant.measured();
    for language in &mut measured.languages {
        let Some(from) = reference
            .languages
            .iter()
            .find(|l| l.language == language.language)
        else {
            continue;
        };
        language.metrics.retain(|metric| {
            category_of(&metric.key).is_some_and(|category| kept.contains(&category))
        });
        language.metrics.extend(
            from.metrics
                .iter()
                .filter(|metric| {
                    category_of(&metric.key).is_some_and(|category| !kept.contains(&category))
                })
                .cloned(),
        );
        if !kept.contains(&A) {
            language.golden_pass_rate = from.golden_pass_rate;
            language.groups.clone_from(&from.groups);
        }
    }
    measured
}

fn category_of(key: &str) -> Option<CategoryId> {
    let id = key.split('.').next()?;
    CategoryId::ALL.into_iter().find(|c| c.as_str() == id)
}

/// How far two scores may differ and still count as the same: measured points are rounded to this in reports.
const SAME: f64 = 0.05;

/// How many points a reference with one feature broken must lose. The plan's target of a total under 65 waits for
/// the pilot to settle the correctness scaling; until then the self-test asks for this loss.
const BROKEN_LOSS: f64 = 15.0;

struct Check {
    name: String,
    holds: bool,
    numbers: String,
}

fn category(score: &LanguageScore, id: CategoryId) -> f64 {
    score
        .categories
        .iter()
        .find(|c| c.category == id)
        .map_or(0.0, |c| c.earned)
}

fn metric(score: &LanguageScore, key: &str) -> f64 {
    score
        .metrics
        .iter()
        .find(|m| m.key == key)
        .map_or(0.0, |m| m.points)
}

fn of(scores: &Scores, variant: Variant) -> &[LanguageScore] {
    &scores
        .iter()
        .find(|(v, _)| *v == variant)
        .expect("every variant is scored")
        .1
}

fn orderings(scores: &Scores, feature: &str) -> Vec<Check> {
    let reference = of(scores, Variant::Reference);
    vec![
        per_language("each reference scores at least 95", reference, |r| {
            (r.total >= 95.0, format!("{:.2}", r.total))
        }),
        loses_only(
            "tests deleted loses only category D",
            reference,
            of(scores, Variant::NoTests),
            D,
        ),
        loses_only(
            "twice the code as dead code loses only category E",
            reference,
            of(scores, Variant::DeadCode),
            E,
        ),
        per_language(
            "tautological tests earn under 1 of the 5 mutation points",
            of(scores, Variant::TautologicalTests),
            |t| {
                let points = metric(t, "D.mutation");
                (points < 1.0, format!("{points:.2}"))
            },
        ),
        broken_loses(
            &format!("{feature} broken scores at least {BROKEN_LOSS} points below its reference"),
            reference,
            of(scores, Variant::BrokenFeature),
        ),
        gains_nothing(
            "a filler doc comment on every public item gains nothing",
            reference,
            of(scores, Variant::FillerDocs),
        ),
    ]
}

fn per_language(
    name: &str,
    scores: &[LanguageScore],
    test: impl Fn(&LanguageScore) -> (bool, String),
) -> Check {
    let mut holds = true;
    let mut numbers = String::new();
    for score in scores {
        let (ok, shown) = test(score);
        holds &= ok;
        let _ = write!(numbers, "{} {shown}, ", score.language.as_str());
    }
    Check {
        name: name.to_string(),
        holds,
        numbers: numbers.trim_end_matches(", ").to_string(),
    }
}

/// The variant scores lower than the reference in `lost`, and within [`SAME`] of it in every other category.
fn loses_only(
    name: &str,
    reference: &[LanguageScore],
    variant: &[LanguageScore],
    lost: CategoryId,
) -> Check {
    let mut holds = true;
    let mut numbers = String::new();
    let mut largest = (0.0f64, String::new());
    for (r, v) in reference.iter().zip(variant) {
        let (before, after) = (category(r, lost), category(v, lost));
        holds &= after < before - SAME;
        let _ = write!(
            numbers,
            "{} {lost:?} {before:.2} -> {after:.2}, ",
            r.language.as_str()
        );
        for id in CategoryId::ALL.into_iter().filter(|id| *id != lost) {
            let change = category(v, id) - category(r, id);
            if change.abs() > largest.0.abs() {
                largest = (change, format!("{id:?} {}", r.language.as_str()));
            }
        }
    }
    holds &= largest.0.abs() <= SAME;
    let place = if largest.1.is_empty() {
        String::new()
    } else {
        format!(" in {}", largest.1)
    };
    let _ = write!(
        numbers,
        "largest change elsewhere {:+.2}{place} (at most {SAME})",
        largest.0
    );
    Check {
        name: name.to_string(),
        holds,
        numbers,
    }
}

/// The broken variant builds and scores at least [`BROKEN_LOSS`] points below the reference in each language.
fn broken_loses(name: &str, reference: &[LanguageScore], broken: &[LanguageScore]) -> Check {
    let mut holds = true;
    let mut numbers = String::new();
    for (r, b) in reference.iter().zip(broken) {
        let built = b.build == benchy::contract::BuildOutcome::Built;
        holds &= built && b.total <= r.total - BROKEN_LOSS;
        let note = if built { "" } else { " (did not build)" };
        numbers.push_str(&format!(
            "{} {:.2} -> {:.2}{note}, ",
            r.language.as_str(),
            r.total,
            b.total
        ));
    }
    Check {
        name: name.to_string(),
        holds,
        numbers: numbers.trim_end_matches(", ").to_string(),
    }
}

/// The variant scores no more than the reference in total and in each category.
fn gains_nothing(name: &str, reference: &[LanguageScore], variant: &[LanguageScore]) -> Check {
    let mut holds = true;
    let mut numbers = String::new();
    for (r, v) in reference.iter().zip(variant) {
        let gained = CategoryId::ALL
            .into_iter()
            .map(|id| (id, category(v, id) - category(r, id)))
            .filter(|(_, change)| *change > SAME)
            .map(|(id, change)| format!(" {id:?} {change:+.2}"))
            .collect::<String>();
        holds &= v.total <= r.total + SAME && gained.is_empty();
        let _ = write!(
            numbers,
            "{} {:.2} -> {:.2}{}, ",
            r.language.as_str(),
            r.total,
            v.total,
            if gained.is_empty() {
                String::new()
            } else {
                format!(" (gains{gained})")
            }
        );
    }
    Check {
        name: name.to_string(),
        holds,
        numbers: numbers.trim_end_matches(", ").to_string(),
    }
}

#[cfg(test)]
mod tests;
