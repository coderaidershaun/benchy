//! Counts what the agents did: replies, tool calls by tool, builds and tests, API errors and operator replies.

use std::collections::HashSet;

use benchy::contract::Activity;
use serde_json::Value;

use super::intervals::{self, Interval};
use super::timestamp::Millis;
use super::transcript::{RecordKind, SYNTHETIC_MODEL, Transcript};

/// A message the operator typed after the kickoff prompt, and how long the run had been waiting for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatorReply {
    pub at: Millis,
    pub waited: Millis,
}

pub fn count_activity(transcripts: &[Transcript], busy_by_agent: &[Vec<Interval>]) -> Activity {
    let mut activity = Activity {
        sessions: count(transcripts.iter().filter(|t| t.agent.is_main_session())),
        subagents: count(transcripts.iter().filter(|t| !t.agent.is_main_session())),
        max_depth: transcripts.iter().map(|t| t.agent.depth).max().unwrap_or(0),
        peak_concurrency: intervals::peak_overlap(busy_by_agent),
        operator_replies: u32::try_from(operator_replies(transcripts).len()).unwrap_or(u32::MAX),
        ..Activity::default()
    };
    let mut seen_replies = HashSet::new();
    let mut seen_tools = HashSet::new();
    for transcript in transcripts {
        let mut error_lines = Vec::new();
        for (position, record) in transcript.records.iter().enumerate() {
            let RecordKind::Assistant(line) = &record.kind else {
                continue;
            };
            if seen_replies.insert(line.message_id.as_str()) {
                if line.api_error.is_some() {
                    activity.api_errors += 1;
                    error_lines.push(position);
                } else if line.model != SYNTHETIC_MODEL {
                    activity.turns += 1;
                }
            }
            // A reply's tool calls can sit on any of its lines, so every line is read; the tool id keeps them unique.
            for tool in &line.tool_uses {
                if !seen_tools.insert(tool.id.as_str()) {
                    continue;
                }
                *activity.tool_calls.entry(tool.name.clone()).or_default() += 1;
                let command = shell_command(&tool.name, &tool.input);
                if command.is_some_and(is_build) {
                    activity.build_invocations += 1;
                }
                if command.is_some_and(is_test) {
                    activity.test_invocations += 1;
                }
            }
        }
        // An error followed by another reply in the same transcript was retried.
        let retried = error_lines.iter().filter(|&&position| {
            transcript.records[position + 1..]
                .iter()
                .any(|r| matches!(&r.kind, RecordKind::Assistant(l) if l.api_error.is_none()))
        });
        activity.retries += count(retried);
    }
    activity
}

/// Operator messages in main sessions after the first, which is the kickoff prompt.
pub fn operator_replies(transcripts: &[Transcript]) -> Vec<OperatorReply> {
    let mut replies = Vec::new();
    for transcript in transcripts.iter().filter(|t| t.agent.is_main_session()) {
        let mut previous: Option<Millis> = None;
        let mut is_kickoff = true;
        for record in &transcript.records {
            let Some(at) = record.timestamp else { continue };
            if matches!(record.kind, RecordKind::Prompt { is_operator: true }) {
                if is_kickoff {
                    is_kickoff = false;
                } else {
                    replies.push(OperatorReply {
                        at,
                        waited: previous.map_or(0, |p| (at - p).max(0)),
                    });
                }
            }
            previous = Some(at);
        }
    }
    replies
}

fn shell_command<'a>(tool: &str, input: &'a Value) -> Option<&'a str> {
    (tool == "Bash").then(|| input.get("command").and_then(Value::as_str))?
}

const BUILD_WORDS: [&str; 5] = [
    "make build",
    "cargo build",
    "go build",
    "dotnet build",
    "dotnet publish",
];
const TEST_WORDS: [&str; 4] = ["make test", "cargo test", "go test", "dotnet test"];

fn is_build(command: &str) -> bool {
    BUILD_WORDS.iter().any(|w| command.contains(w))
}

fn is_test(command: &str) -> bool {
    TEST_WORDS.iter().any(|w| command.contains(w))
}

fn count<T>(items: impl Iterator<Item = T>) -> u32 {
    u32::try_from(items.count()).unwrap_or(u32::MAX)
}
