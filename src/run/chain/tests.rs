//! The whole `benchy run` chain with stand-in programs: a fake harness that commits code and writes a transcript,
//! fixture measured scores in place of the scorer, and a fake judge. The chain must reach `report.md`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use benchy::contract::{Language, MEASURED_FILE, RunStatus};
use serde_json::json;

use super::Chain;
use crate::run::launch::Caps;
use crate::run::layout::OperatorDir;
use crate::run::new::NewRun;

fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn executable(path: &Path, body: &str) {
    fs::write(path, body).expect("the script");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("executable");
}

const FAKE_HARNESS: &str = r#"#!/bin/sh
dir="$CLAUDE_CONFIG_DIR/projects/-ws"
mkdir -p "$dir"
cat > "$dir/11111111-2222-4333-8444-555555555555.jsonl" <<J
{"type":"user","timestamp":"2026-01-01T00:00:00.000Z","version":"2.1.327","cwd":"$PWD","sessionId":"s","message":{"role":"user","content":"Read TASK.md and complete the task."}}
{"type":"assistant","timestamp":"2026-01-01T00:00:05.000Z","version":"2.1.327","cwd":"$PWD","sessionId":"s","message":{"id":"m1","model":"claude-haiku-5-5","content":[{"type":"text","text":"done"}],"usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"cache_creation":{"ephemeral_5m_input_tokens":0,"ephemeral_1h_input_tokens":0}}}}
J
mkdir -p rust/src
printf 'pub fn engine() {}\n' > rust/src/lib.rs
printf '# Delivery\nA stub engine.\n' > DELIVERY.md
git add -A && git commit -q -m done
echo '{"type":"result","subtype":"success","is_error":false,"result":"done","total_cost_usd":0.0001}'
"#;

fn fake_judge(dir: &Path) -> PathBuf {
    let items = [
        "G.self_evidence",
        "G.comment_quality",
        "H.api_design",
        "H.idiom",
        "E.unrequested",
        "I.claims",
    ];
    let judgements: Vec<_> = Language::ALL
        .iter()
        .flat_map(|l| {
            items.iter().map(move |item| {
                json!({ "language": l, "item": item, "level": 2, "evidence": ["rust/src/lib.rs:1"], "reason": "stub" })
            })
        })
        .collect();
    let reply = json!({ "type": "result", "is_error": false, "result": json!({ "judgements": judgements }).to_string() });
    fs::write(dir.join("judge-reply.json"), reply.to_string()).expect("reply");
    let script = dir.join("judge");
    executable(
        &script,
        &format!(
            "#!/bin/sh\ncat '{}'\n",
            dir.join("judge-reply.json").display()
        ),
    );
    script
}

fn fake_checkout(root: &Path) {
    fs::create_dir_all(root.join("validation/judge")).expect("validation");
    for file in [
        "validation/rubric.toml",
        "validation/prices.toml",
        "validation/judge/rubric.toml",
        "validation/judge/prompt.md",
    ] {
        fs::copy(repo(file), root.join(file)).expect("copied");
    }
    fs::copy(
        repo("harnesses/claude-code.toml"),
        root.join("control.toml"),
    )
    .expect("manifest");
}

#[test]
fn a_headless_run_goes_from_launch_to_the_report() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let benchy_root = dir.path().join("checkout");
    fake_checkout(&benchy_root);
    let programs = dir.path().join("programs");
    fs::create_dir_all(&programs).expect("programs");
    let harness = programs.join("claude");
    executable(&harness, FAKE_HARNESS);
    let chain = Chain {
        new_run: NewRun {
            manifest_path: benchy_root.join("control.toml"),
            benchy_root: benchy_root.clone(),
            run_root: dir.path().join("runs"),
            task_pack: Some(repo("task")),
        },
        caps: Caps {
            max_budget_usd: 1.0,
            time_limit: Duration::from_secs(60),
        },
        harness_program: harness,
        judge_program: fake_judge(&programs),
    };
    let done = chain
        .run(|root, id| {
            let target = OperatorDir::new(root, id).dir.join(MEASURED_FILE);
            fs::copy(repo("src/report/fixtures/metrics.json"), target)?;
            Ok(())
        })
        .expect("every step passes");
    let report = fs::read_to_string(done.results.join("report.md")).expect("report.md is written");
    assert!(report.starts_with(&format!("# claude-code run {}", done.id)));
    assert!(done.results.join("scorecard.json").is_file());
    assert!(done.results.join("scorecard.tsv").is_file());
    assert_eq!(
        done.results,
        benchy_root
            .join("results/claude-code")
            .join(done.id.as_str())
    );
    let card = &done.card;
    assert_eq!(
        card.validity.status,
        RunStatus::Valid,
        "{:?}",
        card.validity.notes
    );
    assert_eq!(card.telemetry.activity.turns, 1);
    let judged = card
        .language(Language::Go)
        .expect("go")
        .metrics
        .iter()
        .find(|m| m.key == "H.idiom")
        .expect("idiom");
    assert_eq!(judged.raw, Some(2.0));
    assert!(
        done.finished
            .record
            .finish
            .as_ref()
            .and_then(|f| f.head.clone())
            .is_some()
    );
}
