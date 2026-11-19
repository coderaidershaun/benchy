//! Collector tests on synthetic transcripts: a main session with two parallel subagents, a reply written twice,
//! an API error that was retried, and an operator reply.

use std::fs;
use std::path::{Path, PathBuf};

use benchy::contract::{ModelTokens, Prices, TelemetrySource};

use super::manual::{ManualEntry, ManualError};
use super::{CollectError, collect, cost_gap};

const SESSION: &str = "11111111-2222-4333-8444-555555555555";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/transcripts")
        .join(name)
        .join("projects")
}

/// The prices the fixture transcripts were costed with. They are fixed here so that a change of
/// `validation/prices.toml` does not change what these tests expect.
const FIXTURE_PRICES: &str = r#"
version = "fixture"
status = "provisional"
currency = "USD"
per_tokens = 1000000

[[model]]
id = "claude-opus-5-5"
input = 5.0
output = 25.0
cache_read = 0.5
cache_write_5m = 6.25
cache_write_1h = 10.0

[[model]]
id = "claude-sonnet-5-5"
input = 3.0
output = 15.0
cache_read = 0.3
cache_write_5m = 3.75
cache_write_1h = 6.0

[[model]]
id = "claude-haiku-5-5"
input = 1.0
output = 5.0
cache_read = 0.1
cache_write_5m = 1.25
cache_write_1h = 2.0

[[model]]
id = "claude-fable-5-2"
input = 5.0
output = 25.0
cache_read = 0.5
cache_write_5m = 6.25
cache_write_1h = 10.0

[[model]]
id = "claude-fable-5-1"
input = 5.0
output = 25.0
cache_read = 0.5
cache_write_5m = 6.25
cache_write_1h = 10.0
"#;

fn prices() -> Prices {
    toml::from_str(FIXTURE_PRICES).expect("the fixture prices parse")
}

fn close(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() < 1e-9
}

#[test]
fn each_reply_is_priced_once_across_all_transcripts() {
    let telemetry = collect(&fixture("subagents"), &prices()).expect("the fixture collects");
    // Priced by hand from the fixture's usage. Counting the reply that a2 repeats would add 0.02256.
    assert!(
        close(telemetry.cost_usd, 0.109_851),
        "cost {}",
        telemetry.cost_usd
    );
    assert_eq!(telemetry.transcript_cost_usd, Some(0.1099));
    assert!(cost_gap(&telemetry).expect("there is a transcript total") < 0.01);
    let opus = ModelTokens {
        model: "claude-opus-5-5".to_string(),
        input: 16,
        output: 810,
        thinking: 400,
        cache_read: 63_000,
        cache_write_5m: 1000,
        cache_write_1h: 500,
    };
    assert_eq!(telemetry.tokens[1], opus);
    let models: Vec<&str> = telemetry.tokens.iter().map(|t| t.model.as_str()).collect();
    assert_eq!(
        models,
        ["claude-haiku-5-5", "claude-opus-5-5", "claude-sonnet-5-5"]
    );
    assert_eq!(telemetry.claude_code_versions, ["2.1.327"]);
    assert_eq!(telemetry.source, TelemetrySource::Transcripts);
}

#[test]
fn busy_time_is_the_union_of_every_agent_s_requests_and_tools() {
    let time = collect(&fixture("subagents"), &prices())
        .expect("the fixture collects")
        .time;
    assert!(close(time.busy_seconds, 17.7), "busy {}", time.busy_seconds);
    assert!(
        close(time.agent_seconds, 17.7 + 8.7 + 9.6),
        "agent {}",
        time.agent_seconds
    );
    assert!(close(time.wall_seconds, 78.0));
    assert!(close(time.human_wait_seconds, 60.0));
    assert!(close(time.rate_limit_seconds, 0.5));
    assert_eq!(time.started, "2026-01-01T00:00:00.000Z");
    assert_eq!(time.ended, "2026-01-01T00:01:18.000Z");
}

#[test]
fn agents_carry_their_type_model_parent_and_depth() {
    let telemetry = collect(&fixture("subagents"), &prices()).expect("the fixture collects");
    let summary: Vec<(&str, &str, &str, u32, Option<&str>)> = telemetry
        .agents
        .iter()
        .map(|a| {
            (
                a.id.as_str(),
                a.agent_type.as_str(),
                a.model.as_str(),
                a.depth,
                a.parent.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (SESSION, "main", "claude-opus-5-5", 0, None),
            (
                "a1",
                "general-purpose",
                "claude-sonnet-5-5",
                1,
                Some(SESSION)
            ),
            ("a2", "Explore", "claude-haiku-5-5", 1, Some(SESSION)),
        ]
    );
    let explorer = &telemetry.agents[2];
    assert_eq!(explorer.tokens, 2650 + 2102, "the copied reply is not a2's");
    assert!(close(explorer.busy_seconds, 9.6));
}

#[test]
fn activity_counts_turns_tools_errors_and_operator_replies() {
    let activity = collect(&fixture("subagents"), &prices())
        .expect("the fixture collects")
        .activity;
    assert_eq!(
        (activity.sessions, activity.subagents, activity.max_depth),
        (1, 2, 1)
    );
    assert_eq!(activity.peak_concurrency, 3);
    assert_eq!(activity.turns, 7);
    let tools: Vec<(&str, u32)> = activity
        .tool_calls
        .iter()
        .map(|(k, v)| (k.as_str(), *v))
        .collect();
    assert_eq!(tools, [("Agent", 2), ("Bash", 1), ("Read", 1)]);
    assert_eq!((activity.api_errors, activity.retries), (1, 1));
    assert_eq!(activity.operator_replies, 1);
}

fn one_line_projects(line: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let project = dir.path().join("-x");
    fs::create_dir_all(&project).expect("the project directory");
    fs::write(project.join("s.jsonl"), format!("{line}\n")).expect("the transcript");
    dir
}

#[test]
fn an_unknown_claude_code_version_fails_with_a_clear_message() {
    let line = r#"{"type":"user","timestamp":"2026-01-01T00:00:00Z","version":"3.0.0","message":{"content":"hi"}}"#;
    let dir = one_line_projects(line);
    let error = collect(dir.path(), &prices()).expect_err("version 3.0.0 is not known");
    assert!(matches!(error, CollectError::UnknownVersion { .. }));
    let message = error.to_string();
    assert!(message.contains("Claude Code 3.0.0"), "{message}");
    assert!(message.contains("2.1.316 to 2.1.327"), "{message}");
    assert!(
        message.contains("enter cost, time and agents by hand"),
        "{message}"
    );
}

#[test]
fn a_reply_without_usage_fails_instead_of_costing_nothing() {
    let line = r#"{"type":"assistant","timestamp":"2026-01-01T00:00:00Z","version":"2.1.327","message":{"id":"m","model":"claude-opus-5-5","content":[]}}"#;
    let dir = one_line_projects(line);
    let error = collect(dir.path(), &prices()).expect_err("a reply needs usage");
    assert!(
        error.to_string().contains("has no message.usage"),
        "{error}"
    );
}

#[test]
fn manual_entry_replaces_unreadable_transcripts() {
    let entry: ManualEntry = toml::from_str(
        r#"
        cost_usd = 12.5
        busy_seconds = 3600.0
        [[agent]]
        id = "main"
        agent_type = "main"
        model = "claude-opus-5-5"
        depth = 0
        tokens = 0
        busy_seconds = 3600.0
        started = ""
        ended = ""
        "#,
    )
    .expect("the entry parses");
    let telemetry = entry.apply(None).expect("cost and busy time are enough");
    assert_eq!(telemetry.source, TelemetrySource::Manual);
    assert!(close(telemetry.cost_usd, 12.5));
    assert!(close(telemetry.time.busy_seconds, 3600.0));
    assert_eq!(telemetry.activity.sessions, 1);
    assert_eq!(
        telemetry.manual_entries,
        ["cost_usd", "time.busy_seconds", "agents"]
    );
}

#[test]
fn manual_entry_over_transcripts_is_flagged_as_mixed() {
    let collected = collect(&fixture("subagents"), &prices()).expect("the fixture collects");
    let entry = ManualEntry {
        cost_usd: Some(1.0),
        ..ManualEntry::default()
    };
    let telemetry = entry.apply(Some(collected)).expect("an override applies");
    assert_eq!(telemetry.source, TelemetrySource::Mixed);
    assert!(close(telemetry.cost_usd, 1.0));
    assert!(
        close(telemetry.time.busy_seconds, 17.7),
        "the rest is still collected"
    );
    assert_eq!(telemetry.manual_entries, ["cost_usd"]);
}

#[test]
fn manual_entry_without_transcripts_needs_cost_and_busy_time() {
    let entry = ManualEntry {
        cost_usd: Some(3.0),
        ..ManualEntry::default()
    };
    let error = entry.apply(None).expect_err("busy time is missing");
    assert!(matches!(
        error,
        ManualError::Missing {
            missing: "busy_seconds"
        }
    ));
}
