//! Judge tests with a stand-in `claude` program that replays stored replies: median and spread, a retry after
//! invalid output, a rejected judgement with no citation, and the calibration samples.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use benchy::contract::{Language, Rubric};
use serde_json::json;

use super::{Judge, JudgeError, JudgeRubric};

fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn rubric() -> JudgeRubric {
    let scoring = Rubric::load(&repo("validation/rubric.toml")).expect("valid rubric");
    JudgeRubric::load(&repo("validation/judge"), &scoring).expect("the judge rubric matches")
}

/// A stand-in for `claude -p` that prints reply 1, 2, 3 … in turn, wrapped as Claude Code's JSON output.
fn fake_claude(dir: &Path, replies: &[String]) -> PathBuf {
    for (index, reply) in replies.iter().enumerate() {
        let wrapped = json!({ "type": "result", "is_error": false, "result": reply });
        fs::write(
            dir.join(format!("reply-{}.json", index + 1)),
            wrapped.to_string(),
        )
        .expect("reply");
    }
    let script = dir.join("claude");
    let body = format!(
        "#!/bin/sh\nn=$(cat '{d}/count' 2>/dev/null || echo 0)\nn=$((n+1))\necho $n > '{d}/count'\ncat '{d}/reply-'$n'.json'\n",
        d = dir.display()
    );
    fs::write(&script, body).expect("the script");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("executable");
    script
}

fn calls(dir: &Path) -> u32 {
    fs::read_to_string(dir.join("count")).map_or(0, |t| t.trim().parse().expect("a count"))
}

/// A valid reply giving `level` to every item of `language`, citing `citation`.
fn reply(language: Language, levels: &[(&str, u32)], citation: &str) -> String {
    let judgements: Vec<_> = levels
        .iter()
        .map(|(item, level)| {
            json!({ "language": language, "item": item, "level": level, "evidence": [citation], "reason": "r" })
        })
        .collect();
    format!(
        "Here is my judgement:\n{}",
        json!({ "judgements": judgements })
    )
}

fn uniform(level: u32) -> Vec<(&'static str, u32)> {
    [
        "G.self_evidence",
        "G.comment_quality",
        "H.api_design",
        "H.idiom",
        "E.unrequested",
        "I.claims",
    ]
    .into_iter()
    .map(|k| (k, level))
    .collect()
}

struct Setup {
    _dir: tempfile::TempDir,
    calls: PathBuf,
    workspace: PathBuf,
    judge: Judge,
}

fn setup(replies: &[String]) -> Setup {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let calls = dir.path().join("calls");
    let workspace = dir.path().join("ws");
    fs::create_dir_all(&calls).expect("calls");
    fs::create_dir_all(workspace.join("rust/src")).expect("ws");
    fs::write(
        workspace.join("rust/src/lib.rs"),
        "pub fn a() {}\npub fn b() {}\n",
    )
    .expect("code");
    let program = fake_claude(&calls, replies);
    let judge = Judge {
        program,
        model: "claude-opus-5-5".to_string(),
        samples: 3,
        max_attempts: 3,
        scale_max: 4,
        rubric: rubric(),
    };
    Setup {
        _dir: dir,
        calls,
        workspace,
        judge,
    }
}

#[test]
fn the_median_of_three_judgements_counts_and_the_spread_is_kept() {
    let replies = [1, 4, 3].map(|l| reply(Language::Rust, &uniform(l), "rust/src/lib.rs:2"));
    let setup = setup(&replies);
    let judged = setup
        .judge
        .judge(&setup.workspace, &[Language::Rust])
        .expect("valid judgements");
    let item = judged.item(Language::Rust, "H.idiom").expect("judged");
    assert_eq!(
        (item.levels.clone(), item.median, item.spread),
        (vec![1, 4, 3], 3, 3)
    );
    assert_eq!(item.evidence, ["rust/src/lib.rs:2"]);
    assert_eq!(judged.items.len(), 6);
    assert_eq!(calls(&setup.calls), 3);
}

#[test]
fn invalid_output_is_asked_for_again() {
    let good = reply(Language::Rust, &uniform(2), "rust/src/lib.rs:1");
    let replies = [
        "I think it is fine.".to_string(),
        good.clone(),
        good.clone(),
        good,
    ];
    let setup = setup(&replies);
    let judged = setup
        .judge
        .judge(&setup.workspace, &[Language::Rust])
        .expect("valid after one retry");
    assert_eq!(calls(&setup.calls), 4);
    assert!(judged.items.iter().all(|i| i.median == 2));
}

#[test]
fn a_judgement_with_no_citation_is_rejected_until_the_bound() {
    let mut uncited = uniform(3)
        .iter()
        .map(|(item, level)| json!({ "language": "rust", "item": item, "level": level, "evidence": [], "reason": "r" }))
        .collect::<Vec<_>>();
    uncited.truncate(6);
    let bad = json!({ "judgements": uncited }).to_string();
    let missing_line = reply(Language::Rust, &uniform(3), "rust/src/lib.rs:99");
    let replies = [bad.clone(), missing_line, bad];
    let setup = setup(&replies);
    let error = setup
        .judge
        .judge(&setup.workspace, &[Language::Rust])
        .expect_err("never cited");
    match error {
        JudgeError::GaveUp { attempts, last, .. } => {
            assert_eq!(attempts, 3);
            assert!(last.contains("cites no evidence"), "{last}");
        }
        other => panic!("expected GaveUp, got {other:?}"),
    }
    assert_eq!(calls(&setup.calls), 3);
}

#[derive(serde::Deserialize)]
struct Expected {
    language: Language,
    levels: std::collections::BTreeMap<String, u32>,
}

fn first_file(dir: &Path) -> String {
    for entry in walk(dir) {
        let relative = entry
            .strip_prefix(dir)
            .expect("inside")
            .display()
            .to_string();
        if relative != "expected.toml" && relative != "DELIVERY.md" {
            return relative;
        }
    }
    panic!("the sample {} has no code", dir.display());
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).expect("readable").flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files.sort();
    files
}

/// No paid call: checks each calibration sample is complete, and that a judge replying with the known levels
/// is scored back to exactly those levels. A paid run of the same samples checks the model itself.
#[test]
fn calibration_samples_hold_known_levels_that_the_pipeline_reproduces() {
    let rubric = rubric();
    let samples = walk(&repo("validation/judge/calibration"))
        .into_iter()
        .filter(|p| p.file_name().is_some_and(|n| n == "expected.toml"))
        .collect::<Vec<_>>();
    assert!(samples.len() >= 2, "at least two calibration samples");
    let mut seen_levels = std::collections::BTreeSet::new();
    for expected_path in samples {
        let sample = expected_path.parent().expect("a sample folder");
        let expected: Expected =
            toml::from_str(&fs::read_to_string(&expected_path).expect("readable")).expect("parses");
        let keys: Vec<&String> = expected.levels.keys().collect();
        let mut items: Vec<&String> = rubric.items.iter().map(|i| &i.key).collect();
        items.sort();
        assert_eq!(
            keys,
            items,
            "{} has a known level for every item",
            sample.display()
        );
        assert!(sample.join("DELIVERY.md").is_file());
        let citation = format!("{}:1", first_file(sample));
        let levels: Vec<(&str, u32)> = expected
            .levels
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .collect();
        seen_levels.extend(levels.iter().map(|(_, l)| *l));
        let replies = [0, 1, 2].map(|_| reply(expected.language, &levels, &citation));
        let dir = tempfile::tempdir().expect("temporary");
        let judge = Judge {
            program: fake_claude(dir.path(), &replies),
            model: "claude-opus-5-5".to_string(),
            samples: 3,
            max_attempts: 3,
            scale_max: 4,
            rubric: rubric.clone(),
        };
        let judged = judge
            .judge(sample, &[expected.language])
            .expect("the sample judges");
        for (key, level) in &expected.levels {
            let item = judged.item(expected.language, key).expect("judged");
            assert_eq!(item.median, *level, "{} {key}", sample.display());
        }
    }
    assert!(
        seen_levels.contains(&0) && seen_levels.contains(&4),
        "samples span the scale"
    );
}

#[test]
fn the_prompt_names_every_item_anchor_and_language() {
    let rubric = rubric();
    let prompt = rubric.prompt_for(&Language::ALL);
    assert!(prompt.contains("rust, go, csharp"));
    assert!(prompt.contains("18 in all"));
    for item in &rubric.items {
        assert!(prompt.contains(&item.key));
        for anchor in &item.anchors {
            assert!(prompt.contains(anchor.as_str()));
        }
    }
    assert!(
        !prompt.contains('{') || prompt.contains("\"judgements\""),
        "no placeholder is left"
    );
    assert_eq!(rubric.hash().len(), 16);
}
