//! The report of one run: `report.md`, `scorecard.json` and `scorecard.tsv` in `results/<label>/<run-id>/`.
//! Every point in `report.md` is printed beside the raw value and the thresholds that produced it.

pub mod command;
mod verdict;

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use benchy::contract::{
    CategoryId, Language, LanguageScore, MetricScore, Prices, Rubric, Scorecard, Source,
};

use crate::judge::JudgedScores;

/// What the report is written from. The scorecard holds every number; the rest adds names and evidence.
pub struct ReportInputs<'a> {
    pub card: &'a Scorecard,
    pub rubric: &'a Rubric,
    pub judged: Option<&'a JudgedScores>,
    pub prices: Option<&'a Prices>,
}

/// # Errors
///
/// Fails when a file cannot be written.
pub fn write(results_root: &Path, inputs: &ReportInputs) -> std::io::Result<PathBuf> {
    let card = inputs.card;
    let dir = results_root.join(&card.label).join(&card.run_id);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("report.md"), render(inputs))?;
    let json = serde_json::to_string_pretty(card).expect("a scorecard always serialises");
    fs::write(dir.join("scorecard.json"), json + "\n")?;
    fs::write(dir.join("scorecard.tsv"), card.to_tsv())?;
    Ok(dir)
}

pub fn render(inputs: &ReportInputs) -> String {
    let mut out = String::new();
    header(&mut out, inputs);
    scoreboard(&mut out, inputs.card);
    let _ = writeln!(
        out,
        "## Verdict\n\n{}\n",
        verdict::sentences(inputs).join(" ")
    );
    breakdown(&mut out, inputs);
    category_detail(&mut out, inputs);
    correctness_map(&mut out, inputs.card);
    metric_section(&mut out, inputs, "Tests", CategoryId::D);
    metric_section(&mut out, inputs, "Size and economy", CategoryId::E);
    comment_density(&mut out, inputs);
    run_profile(&mut out, inputs);
    judge_findings(&mut out, inputs);
    caveats(&mut out, inputs.card);
    reproduce(&mut out, inputs.card);
    out
}

fn header(out: &mut String, inputs: &ReportInputs) {
    let card = inputs.card;
    let v = &card.versions;
    let _ = writeln!(out, "# {} run {}\n", card.label, card.run_id);
    let rows = [
        ("Date", card.date.clone()),
        ("Task version", v.task.clone()),
        (
            "Rubric version",
            format!("{} ({:?})", v.rubric, inputs.rubric.status),
        ),
        ("Judge model", v.judge_model.clone()),
        ("Claude Code", v.claude_code.clone()),
        ("Model", card.manifest.claude_code.model.clone()),
        ("Auth mode", json_name(&card.manifest.auth)),
        ("Isolation level", json_name(&card.isolation)),
        ("Validity", json_name(&card.validity.status)),
        ("Completion", json_name(&card.validity.completion)),
        ("Audit flags", card.validity.audit_flags.to_string()),
        (
            "Operator replies",
            card.validity.operator_replies.to_string(),
        ),
    ];
    table(
        out,
        &["Field", "Value"],
        rows.iter().map(|(k, v)| vec![k.to_string(), v.clone()]),
    );
}

fn scoreboard(out: &mut String, card: &Scorecard) {
    let h = &card.headline;
    let _ = writeln!(out, "## Scoreboard\n");
    let spread = "no baseline yet";
    let lowest = h
        .lowest_language
        .map_or(String::new(), |l| format!(", lowest {}", l.as_str()));
    let rows = vec![
        vec![
            "Quality".to_string(),
            format!(
                "{} (mean {}{lowest} {})",
                opt(h.quality),
                opt(h.quality_mean),
                opt(h.quality_lowest)
            ),
            spread.to_string(),
        ],
        vec![
            "Cost".to_string(),
            format!("{:.2} USD", h.cost_usd),
            spread.to_string(),
        ],
        vec![
            "Elapsed busy time".to_string(),
            duration(h.busy_seconds),
            spread.to_string(),
        ],
        vec![
            "Overall".to_string(),
            opt(h.overall),
            absent_reason(h.overall),
        ],
        vec![
            "Efficiency".to_string(),
            opt(h.efficiency),
            absent_reason(h.efficiency),
        ],
        vec![
            "Agents".to_string(),
            card.telemetry.agents.len().to_string(),
            String::new(),
        ],
        vec![
            "Peak concurrency".to_string(),
            card.telemetry.activity.peak_concurrency.to_string(),
            String::new(),
        ],
        vec![
            "Operator replies".to_string(),
            card.validity.operator_replies.to_string(),
            String::new(),
        ],
    ];
    table(
        out,
        &["Measure", "Value", "Baseline spread"],
        rows.into_iter(),
    );
}

fn absent_reason(value: Option<f64>) -> String {
    match value {
        Some(_) => String::new(),
        None => "absent until the pilot sets the rubric baselines".to_string(),
    }
}

fn breakdown(out: &mut String, inputs: &ReportInputs) {
    let card = inputs.card;
    let _ = writeln!(out, "## Score breakdown\n");
    let mut rows = Vec::new();
    for category in &inputs.rubric.categories {
        let earned: Vec<f64> = Language::ALL
            .iter()
            .map(|&l| earned(card.language(l), category.id))
            .collect();
        let mut row = vec![format!("{} {}", category.id.as_str(), category.name)];
        row.extend(earned.iter().map(|e| points(*e)));
        row.push(points(earned.iter().sum::<f64>() / 3.0));
        row.push(points(category.points));
        rows.push(row);
    }
    let mut total = vec!["Total".to_string()];
    let totals: Vec<f64> = Language::ALL
        .iter()
        .map(|&l| card.language(l).map_or(0.0, |s| s.total))
        .collect();
    total.extend(totals.iter().map(|t| points(*t)));
    total.push(points(totals.iter().sum::<f64>() / 3.0));
    total.push(points(100.0));
    rows.push(total);
    let mut factor = vec!["Correctness factor".to_string()];
    factor.extend(Language::ALL.iter().map(|&l| {
        card.language(l)
            .map_or(String::new(), |s| format!("{:.3}", s.correctness_factor))
    }));
    factor.extend([String::new(), String::new()]);
    rows.push(factor);
    table(
        out,
        &["Category", "Rust", "Go", "C#", "Mean", "Maximum"],
        rows.into_iter(),
    );
}

fn earned(score: Option<&LanguageScore>, category: CategoryId) -> f64 {
    score
        .and_then(|s| s.categories.iter().find(|c| c.category == category))
        .map_or(0.0, |c| c.earned)
}

fn category_detail(out: &mut String, inputs: &ReportInputs) {
    let _ = writeln!(out, "## Category detail\n");
    let _ = writeln!(
        out,
        "Each cell is raw value / points. Full points at the full threshold, none at the zero threshold. \
         Points are after the build gate and the correctness scaling.\n"
    );
    for category in &inputs.rubric.categories {
        let _ = writeln!(
            out,
            "### {} {} ({} points)\n",
            category.id.as_str(),
            category.name,
            points(category.points)
        );
        let rows = category.metrics.iter().map(|metric| {
            let key = format!("{}.{}", category.id.as_str(), metric.id);
            let mut row = vec![
                format!("{} ({})", metric.name, json_name(&metric.source)),
                format!(
                    "{} / {} ({})",
                    number(metric.full),
                    number(metric.zero),
                    json_name(&metric.scale)
                ),
            ];
            row.extend(
                Language::ALL
                    .iter()
                    .map(|&l| cell(metric_of(inputs.card, l, &key))),
            );
            row.push(points(metric.points));
            row
        });
        table(
            out,
            &["Metric", "Full / zero", "Rust", "Go", "C#", "Max"],
            rows,
        );
    }
}

fn metric_of<'a>(card: &'a Scorecard, language: Language, key: &str) -> Option<&'a MetricScore> {
    card.language(language)?
        .metrics
        .iter()
        .find(|m| m.key == key)
}

fn cell(metric: Option<&MetricScore>) -> String {
    match metric {
        Some(m) => format!(
            "{} / {}",
            m.raw.map_or("not measured".to_string(), number),
            points(m.points)
        ),
        None => "not measured / 0.00".to_string(),
    }
}

fn correctness_map(out: &mut String, card: &Scorecard) {
    let _ = writeln!(out, "## Correctness map\n");
    let rows = Language::ALL.iter().map(|&l| {
        let score = card.language(l);
        let raw = |key: &str| {
            metric_of(card, l, key)
                .and_then(|m| m.raw)
                .map_or("not measured".to_string(), number)
        };
        vec![
            l.as_str().to_string(),
            score.map_or(String::new(), |s| json_name(&s.build)),
            score.map_or(String::new(), |s| {
                format!("{:.1}%", s.golden_pass_rate * 100.0)
            }),
            raw("A.fuzz"),
            raw("A.invariants"),
        ]
    });
    table(
        out,
        &[
            "Language",
            "Build",
            "Golden pass rate",
            "Fuzz agreement",
            "Invariants",
        ],
        rows,
    );
    group_table(out, card);
}

/// Golden sessions passed out of all in each feature group, one column for each language.
fn group_table(out: &mut String, card: &Scorecard) {
    let groups: std::collections::BTreeSet<&str> = card
        .languages
        .iter()
        .flat_map(|l| l.groups.iter().map(|g| g.group.as_str()))
        .collect();
    if groups.is_empty() {
        let _ = writeln!(out, "Pass rates by feature group were not measured.\n");
        return;
    }
    let _ = writeln!(
        out,
        "Golden sessions passed out of all, by feature group. The golden pass rate is the mean of the group rates.\n"
    );
    let rows = groups.into_iter().map(|group| {
        let mut row = vec![group.to_string()];
        for language in Language::ALL {
            let rate = card
                .language(language)
                .and_then(|l| l.groups.iter().find(|g| g.group == group));
            row.push(rate.map_or("not measured".to_string(), |g| {
                let mark = if g.passed == g.total { "" } else { " ✗" };
                format!("{} / {}{mark}", g.passed, g.total)
            }));
        }
        row
    });
    table(out, &["Feature group", "Rust", "Go", "C#"], rows);
}

/// The measured comment density of each language, and whether the rubric's cap applied.
fn comment_density(out: &mut String, inputs: &ReportInputs) {
    let cap = &inputs.rubric.comment_density;
    let _ = writeln!(
        out,
        "Comment density is the share of code and comment lines that hold only a comment. Above {:.0}%, `{}` earns at most {:.0}% of its points.\n",
        cap.density * 100.0,
        cap.metric,
        cap.factor * 100.0
    );
    let rows = Language::ALL.iter().map(|&language| {
        let density = inputs
            .card
            .language(language)
            .and_then(|l| l.comment_density);
        vec![
            language.as_str().to_string(),
            density.map_or("not measured".to_string(), |d| format!("{:.1}%", d * 100.0)),
            match density {
                Some(d) if d > cap.density => "yes".to_string(),
                Some(_) => "no".to_string(),
                None => String::new(),
            },
        ]
    });
    table(out, &["Language", "Comment density", "Over the cap"], rows);
}

fn metric_section(out: &mut String, inputs: &ReportInputs, title: &str, category: CategoryId) {
    let _ = writeln!(out, "## {title}\n");
    let rows = inputs
        .rubric
        .category(category)
        .metrics
        .iter()
        .map(|metric| {
            let key = format!("{}.{}", category.as_str(), metric.id);
            let mut row = vec![metric.name.clone()];
            row.extend(Language::ALL.iter().map(|&l| {
                metric_of(inputs.card, l, &key)
                    .and_then(|m| m.raw)
                    .map_or("not measured".to_string(), number)
            }));
            row.push(metric.measure.clone());
            row
        });
    table(
        out,
        &["Measure", "Rust", "Go", "C#", "What the value is"],
        rows,
    );
}

fn run_profile(out: &mut String, inputs: &ReportInputs) {
    let telemetry = &inputs.card.telemetry;
    let _ = writeln!(out, "## Run profile\n");
    let time = &telemetry.time;
    let rows = [
        ("Started", time.started.clone()),
        ("Ended", time.ended.clone()),
        ("Busy time (scored)", duration(time.busy_seconds)),
        ("Wall-clock time", duration(time.wall_seconds)),
        ("Summed agent time", duration(time.agent_seconds)),
        ("Lost to retries", duration(time.rate_limit_seconds)),
        ("Waiting for a human", duration(time.human_wait_seconds)),
        (
            "Cost in Claude Code",
            format!("{:.4} USD", telemetry.cost_usd),
        ),
        (
            "Transcripts' own total",
            telemetry
                .transcript_cost_usd
                .map_or("none".to_string(), |c| format!("{c:.4} USD")),
        ),
        (
            "Spend outside Claude Code",
            format!("{:.2} USD", telemetry.external_spend_usd),
        ),
        ("Turns", telemetry.activity.turns.to_string()),
        (
            "API errors / retries",
            format!(
                "{} / {}",
                telemetry.activity.api_errors, telemetry.activity.retries
            ),
        ),
        (
            "Builds / test runs",
            format!(
                "{} / {}",
                telemetry.activity.build_invocations, telemetry.activity.test_invocations
            ),
        ),
    ];
    table(
        out,
        &["Measure", "Value"],
        rows.iter().map(|(k, v)| vec![k.to_string(), v.clone()]),
    );
    let _ = writeln!(out, "### Tokens and cost by model\n");
    let token_rows = telemetry.tokens.iter().map(|t| {
        let cost = inputs
            .prices
            .and_then(|p| p.cost_usd(t).ok())
            .map_or("unpriced".to_string(), |c| format!("{c:.4}"));
        vec![
            t.model.clone(),
            t.input.to_string(),
            t.output.to_string(),
            t.thinking.to_string(),
            t.cache_read.to_string(),
            t.cache_write_5m.to_string(),
            t.cache_write_1h.to_string(),
            cost,
        ]
    });
    table(
        out,
        &[
            "Model",
            "Input",
            "Output",
            "Thinking",
            "Cache read",
            "Cache write 5m",
            "Cache write 1h",
            "USD",
        ],
        token_rows,
    );
    let _ = writeln!(out, "### Agent tree\n");
    let agent_rows = telemetry.agents.iter().map(|a| {
        vec![
            format!("{}{}", "  ".repeat(a.depth as usize), a.id),
            a.agent_type.clone(),
            a.model.clone(),
            a.tokens.to_string(),
            duration(a.busy_seconds),
            a.started.clone(),
        ]
    });
    table(
        out,
        &["Agent", "Type", "Model", "Tokens", "Busy", "Started"],
        agent_rows,
    );
    let _ = writeln!(out, "### Tool mix\n");
    let tool_rows = telemetry
        .activity
        .tool_calls
        .iter()
        .map(|(tool, count)| vec![tool.clone(), count.to_string()]);
    table(out, &["Tool", "Calls"], tool_rows);
}

fn judge_findings(out: &mut String, inputs: &ReportInputs) {
    let _ = writeln!(out, "## Judge findings\n");
    let Some(judged) = inputs.judged else {
        let _ = writeln!(out, "Not judged: every judged item scores 0.\n");
        return;
    };
    let _ = writeln!(
        out,
        "Model {}, {} judgements per item; the median counts.\n",
        judged.model, judged.samples
    );
    let mut rows = Vec::new();
    for category in &inputs.rubric.categories {
        for metric in category
            .metrics
            .iter()
            .filter(|m| m.source == Source::Judged)
        {
            let key = format!("{}.{}", category.id.as_str(), metric.id);
            for language in Language::ALL {
                if let Some(item) = judged.item(language, &key) {
                    rows.push(vec![
                        metric.name.clone(),
                        language.as_str().to_string(),
                        format!("{} of 4", item.median),
                        item.spread.to_string(),
                        item.evidence.join(", "),
                        item.reason.replace('|', "/"),
                    ]);
                }
            }
        }
    }
    table(
        out,
        &["Item", "Language", "Level", "Spread", "Evidence", "Reason"],
        rows.into_iter(),
    );
}

fn caveats(out: &mut String, card: &Scorecard) {
    let _ = writeln!(out, "## Validity and caveats\n");
    let mut lines = vec![
        format!("Status: {}.", json_name(&card.validity.status)),
        format!("Completion: {}.", json_name(&card.validity.completion)),
        format!(
            "Uncommitted work at the end: {}.",
            if card.validity.uncommitted_work {
                "yes, and it is not scored"
            } else {
                "none"
            }
        ),
        format!("Telemetry source: {}.", json_name(&card.telemetry.source)),
        "This conclusion rests on 1 run.".to_string(),
    ];
    lines.extend(card.validity.notes.iter().cloned());
    for line in lines {
        let _ = writeln!(out, "- {line}");
    }
    out.push('\n');
}

fn reproduce(out: &mut String, card: &Scorecard) {
    let _ = writeln!(out, "## Reproduce\n");
    let v = &card.versions;
    let _ = writeln!(
        out,
        "- Commands: `benchy score {id}`, `benchy judge {id}`, `benchy report {id}`",
        id = card.run_id
    );
    let _ = writeln!(out, "- Workspace hash: `{}`", v.workspace);
    let _ = writeln!(
        out,
        "- Scorer image: `{}`",
        v.scorer_image
            .as_deref()
            .unwrap_or("none, scored on the host")
    );
    let _ = writeln!(out, "- Benchy version: `{}`", v.benchy);
}

fn table(out: &mut String, header: &[&str], rows: impl Iterator<Item = Vec<String>>) {
    let _ = writeln!(out, "| {} |", header.join(" | "));
    let _ = writeln!(out, "|{}", "---|".repeat(header.len()));
    for row in rows {
        let _ = writeln!(out, "| {} |", row.join(" | "));
    }
    out.push('\n');
}

pub(crate) fn points(value: f64) -> String {
    format!("{value:.2}")
}

pub(crate) fn number(value: f64) -> String {
    let text = format!("{value:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_string()
    } else {
        text.to_string()
    }
}

fn opt(value: Option<f64>) -> String {
    value.map_or("absent".to_string(), points)
}

fn duration(seconds: f64) -> String {
    let total = seconds.round() as u64;
    format!(
        "{}h {:02}m {:02}s",
        total / 3600,
        total / 60 % 60,
        total % 60
    )
}

pub(crate) fn json_name<T: serde::Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => name,
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests;
