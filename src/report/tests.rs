//! Report tests: a fixture run aggregates to the stored scorecard and renders to the stored report.
//! Run with `BENCHY_BLESS=1` to rewrite the stored files after a deliberate change.

use std::fs;
use std::path::{Path, PathBuf};

use benchy::contract::{HarnessManifest, MeasuredScores, Prices, Rubric, Scorecard, Telemetry};

use super::{ReportInputs, render};
use crate::aggregate::{Inputs, aggregate};
use crate::judge::JudgedScores;
use crate::run::record::RunRecord;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/report/fixtures")
        .join(name)
}

fn json<T: serde::de::DeserializeOwned>(name: &str) -> T {
    let text = fs::read_to_string(fixture(name)).expect("the fixture is readable");
    serde_json::from_str(&text).expect("the fixture parses")
}

fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

pub(crate) fn fixture_card() -> (Scorecard, Rubric, JudgedScores) {
    let rubric = Rubric::load(&repo("validation/rubric.toml")).expect("valid rubric");
    let measured: MeasuredScores = json("metrics.json");
    let judged: JudgedScores = json("judge.json");
    let telemetry: Telemetry = json("telemetry.json");
    let record: RunRecord = json("run.json");
    let manifest = HarnessManifest::load(&repo("harnesses/claude-code.toml")).expect("valid");
    let card = aggregate(&Inputs {
        rubric: &rubric,
        measured: &measured,
        judged: Some(&judged),
        telemetry: &telemetry,
        manifest: &manifest,
        record: &record,
        review: None,
        task_version: "unversioned",
    });
    (card, rubric, judged)
}

fn check_stored(name: &str, actual: &str) {
    let path = fixture(name);
    if std::env::var_os("BENCHY_BLESS").is_some() {
        fs::write(&path, actual).expect("the stored file is writable");
    }
    let stored = fs::read_to_string(&path).expect("the stored file exists; bless it once");
    assert!(
        stored == actual,
        "{name} differs from the stored copy; diff it, then bless with BENCHY_BLESS=1"
    );
}

#[test]
fn the_fixture_run_aggregates_to_the_stored_scorecard() {
    let (mut card, ..) = fixture_card();
    card.versions.benchy = "fixture".to_string();
    let text = serde_json::to_string_pretty(&card).expect("serialises") + "\n";
    check_stored("scorecard.json", &text);
}

#[test]
fn the_fixture_scorecard_renders_the_stored_report() {
    let (mut card, rubric, judged) = fixture_card();
    card.versions.benchy = "fixture".to_string();
    let prices = Prices::load(&repo("validation/prices.toml")).expect("valid prices");
    let report = render(&ReportInputs {
        card: &card,
        rubric: &rubric,
        judged: Some(&judged),
        prices: Some(&prices),
    });
    let headings: Vec<&str> = report.lines().filter(|l| l.starts_with("## ")).collect();
    assert_eq!(
        headings,
        [
            "## Scoreboard",
            "## Verdict",
            "## Score breakdown",
            "## Category detail",
            "## Correctness map",
            "## Tests",
            "## Size and economy",
            "## Run profile",
            "## Judge findings",
            "## Validity and caveats",
            "## Reproduce"
        ],
        "with the title line as the header, these are the twelve sections of the plan"
    );
    check_stored("report.md", &report);
}

#[test]
fn the_tsv_keeps_its_fixed_column_order() {
    let (card, ..) = fixture_card();
    let tsv = card.to_tsv();
    let header: Vec<&str> = tsv.lines().next().expect("a header").split('\t').collect();
    assert_eq!(&header[..4], ["run_id", "label", "date", "task_version"]);
    assert_eq!(&header[24..27], ["rust_total", "go_total", "csharp_total"]);
    assert_eq!(header[27], "rust_A");
    assert_eq!(*header.last().expect("columns"), "csharp_I");
    let row: Vec<&str> = tsv.lines().nth(1).expect("a row").split('\t').collect();
    assert_eq!(row.len(), header.len());
    assert_eq!(
        row[header
            .iter()
            .position(|c| *c == "csharp_A")
            .expect("column")],
        "0.00"
    );
}

#[test]
fn every_point_in_the_fixture_traces_to_a_metric() {
    let (card, ..) = fixture_card();
    for language in &card.languages {
        let from_metrics: f64 = language.metrics.iter().map(|m| m.points).sum();
        assert!(
            (from_metrics - language.total).abs() < 1e-9,
            "{:?}",
            language.language
        );
        for metric in &language.metrics {
            assert!(
                metric.points <= metric.max + 1e-9 && metric.points >= 0.0,
                "{metric:?}"
            );
        }
    }
}
