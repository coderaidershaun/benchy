//! The verdict: five sentences generated from the scorecard, never written by hand.

use benchy::contract::{BuildOutcome, Language, RunStatus};

use super::{ReportInputs, number, points};

/// Against the baseline, the strongest language and category, then the three largest point losses with their cause.
pub fn sentences(inputs: &ReportInputs) -> Vec<String> {
    let card = inputs.card;
    if card.validity.status == RunStatus::Void {
        return vec![
            "The run is void: the harness read benchy material, so it has no score.".to_string(),
        ];
    }
    let mut out = vec![
        "No baseline spread is published yet, so this run cannot be called better or worse than the baseline."
            .to_string(),
    ];
    let strongest = card
        .languages
        .iter()
        .max_by(|a, b| a.total.total_cmp(&b.total))
        .expect("a scorecard has three languages");
    let best_category = strongest
        .categories
        .iter()
        .max_by(|a, b| (a.earned / a.max).total_cmp(&(b.earned / b.max)))
        .expect("a language has nine categories");
    out.push(format!(
        "The strongest language was {} with {} of 100, and its best category was {} {} with {} of {}.",
        name(strongest.language),
        points(strongest.total),
        best_category.category.as_str(),
        inputs.rubric.category(best_category.category).name,
        points(best_category.earned),
        points(best_category.max),
    ));
    let gate = &inputs.rubric.build_gate.categories;
    let mut losses: Vec<Loss> = card
        .languages
        .iter()
        .flat_map(|l| {
            let failed = l.build == BuildOutcome::Failed;
            l.metrics.iter().map(move |m| Loss {
                lost: m.max - m.points,
                language: l.language,
                key: &m.key,
                raw: m.raw,
                factor: l.correctness_factor,
                gated: failed && gate.iter().any(|c| m.key.starts_with(c.as_str())),
            })
        })
        .filter(|loss| loss.lost > 1e-9)
        .collect();
    losses.sort_by(|a, b| b.lost.total_cmp(&a.lost).then(a.key.cmp(b.key)));
    for (rank, loss) in losses.iter().take(3).enumerate() {
        let metric_name = inputs
            .rubric
            .metric(loss.key)
            .map_or(loss.key, |m| m.name.as_str());
        let place = ["largest", "second largest", "third largest"][rank];
        out.push(format!(
            "The {place} loss was {} points on {metric_name} in {}, because {}.",
            points(loss.lost),
            name(loss.language),
            loss.cause(),
        ));
    }
    out
}

fn name(language: Language) -> &'static str {
    match language {
        Language::Rust => "Rust",
        Language::Go => "Go",
        Language::CSharp => "C#",
    }
}

/// Points one metric of one language fell short of its maximum, and what explains it.
struct Loss<'a> {
    lost: f64,
    language: Language,
    key: &'a str,
    raw: Option<f64>,
    factor: f64,
    gated: bool,
}

impl Loss<'_> {
    fn cause(&self) -> String {
        match self.raw {
            _ if self.gated => "the language did not build from a clean checkout".to_string(),
            None => "it was not measured".to_string(),
            Some(raw) if self.factor < 1.0 => format!(
                "it measured {} and the correctness factor of {:.3} scaled it",
                number(raw),
                self.factor
            ),
            Some(raw) => format!("it measured {}", number(raw)),
        }
    }
}
