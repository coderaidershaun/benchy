//! Audit tests: tool calls inside the run pass, and each way of reaching out of it is flagged.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{AuditScope, FindingKind, audit};
use crate::collect::transcript::{
    self, AgentInfo, AssistantLine, Record, RecordKind, ToolUse, Transcript, Usage,
};

fn scope() -> AuditScope {
    AuditScope {
        run_dir: PathBuf::from("/r/abc"),
        home: PathBuf::from("/home/u"),
        benchy_root: PathBuf::from("/home/u/code/benchy"),
    }
}

fn one_call(tool: &str, input: Value, cwd: &str) -> Transcript {
    let line = AssistantLine {
        message_id: "m".to_string(),
        model: "claude-opus-5-5".to_string(),
        usage: Usage::default(),
        request_ref: None,
        tool_uses: vec![ToolUse {
            id: "t".to_string(),
            name: tool.to_string(),
            input,
        }],
        api_error: None,
    };
    Transcript {
        path: PathBuf::from("s.jsonl"),
        agent: AgentInfo {
            id: "s".to_string(),
            session_id: "s".to_string(),
            parent: None,
            agent_type: "main".to_string(),
            model: None,
            depth: 0,
        },
        records: vec![Record {
            line: 1,
            timestamp: Some(0),
            cwd: Some(cwd.to_string()),
            version: None,
            kind: RecordKind::Assistant(line),
        }],
    }
}

fn kinds(tool: &str, input: Value, cwd: &str) -> Vec<FindingKind> {
    audit(&[one_call(tool, input, cwd)], &scope())
        .findings
        .iter()
        .map(|f| f.kind)
        .collect()
}

#[test]
fn a_tool_call_that_climbs_out_of_the_run_directory_is_flagged() {
    let projects =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transcripts/subagents/projects");
    let transcripts = transcript::read_projects(&projects).expect("the fixture reads");
    let report = audit(&transcripts, &scope());
    let outside: Vec<(&str, &str)> = report
        .findings
        .iter()
        .filter(|f| f.kind == FindingKind::OutsideRun)
        .map(|f| (f.tool.as_str(), f.evidence.as_str()))
        .collect();
    // Once in a1 and once where a2 repeats that reply; the Read inside the workspace is not flagged.
    assert_eq!(
        outside,
        [("Bash", "../../secret.txt"), ("Bash", "../../secret.txt")]
    );
    assert_eq!(report.findings.len(), 2);
    assert_eq!(report.tool_calls_checked, 5);
}

#[test]
fn work_inside_the_run_and_on_the_toolchain_is_clean() {
    let calls = [
        ("Read", json!({ "file_path": "/r/abc/ws/src/main.rs" })),
        (
            "Bash",
            json!({ "command": "cargo build --release 2>/dev/null && ls /usr/bin/make" }),
        ),
        ("Bash", json!({ "command": "ls ~/.cargo/registry" })),
        ("Grep", json!({ "pattern": "fn main", "path": "rust/src" })),
    ];
    for (tool, input) in calls {
        assert_eq!(
            kinds(tool, input.clone(), "/r/abc/ws"),
            [],
            "{tool} {input}"
        );
    }
}

#[test]
fn a_path_into_the_benchy_checkout_is_the_most_serious_finding() {
    let found = kinds(
        "Read",
        json!({ "file_path": "../../../home/u/code/benchy/validation/rubric.toml" }),
        "/r/abc/ws",
    );
    assert!(found.contains(&FindingKind::Benchy), "{found:?}");
    let named = kinds(
        "Bash",
        json!({ "command": "find . -name '*benchy*'" }),
        "/r/abc/ws",
    );
    assert_eq!(named, [FindingKind::Benchy]);
}

#[test]
fn disk_wide_searches_network_and_a_foreign_working_directory_are_flagged() {
    let search = kinds(
        "Bash",
        json!({ "command": "grep -r rubric ~" }),
        "/r/abc/ws",
    );
    assert!(search.contains(&FindingKind::DiskSearch), "{search:?}");
    let clone = kinds(
        "Bash",
        json!({ "command": "git clone https://example.com/x.git" }),
        "/r/abc/ws",
    );
    assert_eq!(clone, [FindingKind::Network]);
    let fetch = kinds(
        "WebFetch",
        json!({ "url": "https://example.com" }),
        "/r/abc/ws",
    );
    assert_eq!(fetch, [FindingKind::Network]);
    let elsewhere = kinds("Read", json!({ "file_path": "notes.md" }), "/home/u");
    assert!(
        elsewhere.contains(&FindingKind::WorkingDirectory),
        "{elsewhere:?}"
    );
}
