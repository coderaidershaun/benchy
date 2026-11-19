//! The telemetry collector: reads every transcript of a run's configuration directory and derives cost, time and agents.
//! It needs no help from the harness, so any harness can be measured the same way.

mod activity;
pub mod intervals;
pub mod manual;
pub mod timestamp;
pub mod transcript;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use benchy::contract::{
    Agent, ModelTokens, Prices, PricesError, RunTime, Telemetry, TelemetrySource,
};

use intervals::Interval;
use timestamp::Millis;
use transcript::{AssistantLine, RecordKind, SYNTHETIC_MODEL, Transcript};

pub use activity::count_activity;

#[derive(Debug, thiserror::Error)]
pub enum CollectError {
    #[error("failed to read {path}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} line {line} is not JSON")]
    Json {
        path: String,
        line: usize,
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "{path} line {line} was written by Claude Code {version}; this collector knows the transcript shape of \
         Claude Code {} to {} only. Check the shape, then widen the known range, or enter cost, time and agents by hand",
        transcript::version_text(transcript::OLDEST_KNOWN_VERSION),
        transcript::version_text(transcript::NEWEST_KNOWN_VERSION)
    )]
    UnknownVersion {
        path: String,
        line: usize,
        version: String,
    },
    #[error(
        "{path} line {line} (Claude Code {version}) has no {what}; the transcript shape changed, so the collector \
         cannot trust its numbers. Enter cost, time and agents by hand"
    )]
    Shape {
        path: String,
        line: usize,
        version: String,
        what: String,
    },
    #[error("transcript file {path} is in a place this collector does not know")]
    Layout { path: String },
    #[error("no transcripts under {path}; did the harness run with this configuration directory?")]
    Empty { path: String },
    #[error("failed to price the run's tokens")]
    Price(#[from] PricesError),
}

/// Telemetry of every transcript under `<config>/projects`. External spend is left at zero for the caller to add.
///
/// # Errors
///
/// Fails on an unknown transcript version or shape, an unreadable file, no transcripts at all, or a model with no price.
pub fn collect(projects: &Path, prices: &Prices) -> Result<Telemetry, CollectError> {
    let transcripts = transcript::read_projects(projects)?;
    if transcripts.is_empty() {
        return Err(CollectError::Empty {
            path: projects.display().to_string(),
        });
    }
    let replies = unique_replies(&transcripts);
    let tokens = tokens_by_model(&replies);
    let mut cost_usd = 0.0;
    for model_tokens in &tokens {
        cost_usd += prices.cost_usd(model_tokens)?;
    }
    let busy_by_agent: Vec<Vec<Interval>> = transcripts
        .iter()
        .enumerate()
        .map(|(index, transcript)| {
            let owned: HashSet<&str> = replies
                .iter()
                .filter(|(i, _)| *i == index)
                .map(|(_, r)| r.message_id.as_str())
                .collect();
            busy_intervals(transcript, &owned)
        })
        .collect();
    let agents = agents(&transcripts, &replies, &busy_by_agent);
    let time = run_time(&transcripts, &busy_by_agent);
    let activity = count_activity(&transcripts, &busy_by_agent);
    Ok(Telemetry {
        source: TelemetrySource::Transcripts,
        claude_code_versions: versions(&transcripts),
        cost_usd,
        transcript_cost_usd: transcript_cost(&transcripts),
        external_spend_usd: 0.0,
        time,
        tokens,
        agents,
        activity,
        manual_entries: Vec::new(),
    })
}

/// How far the collector's cost is from the transcripts' own total, as a share of that total.
pub fn cost_gap(telemetry: &Telemetry) -> Option<f64> {
    let theirs = telemetry.transcript_cost_usd?;
    (theirs > 0.0).then(|| (telemetry.cost_usd - theirs).abs() / theirs)
}

/// The first line of each API reply, with the index of the transcript it was found in.
/// The same reply is written once per content block and can appear in more than one file.
fn unique_replies(transcripts: &[Transcript]) -> Vec<(usize, &AssistantLine)> {
    let mut seen = HashSet::new();
    let mut replies = Vec::new();
    for (index, transcript) in transcripts.iter().enumerate() {
        for record in &transcript.records {
            if let RecordKind::Assistant(line) = &record.kind
                && seen.insert(line.message_id.as_str())
            {
                replies.push((index, line));
            }
        }
    }
    replies
}

fn tokens_by_model(replies: &[(usize, &AssistantLine)]) -> Vec<ModelTokens> {
    let mut by_model: BTreeMap<&str, ModelTokens> = BTreeMap::new();
    for (_, reply) in replies {
        if reply.model == SYNTHETIC_MODEL {
            continue;
        }
        let entry = by_model.entry(&reply.model).or_insert_with(|| ModelTokens {
            model: reply.model.clone(),
            ..ModelTokens::default()
        });
        let usage = reply.usage;
        entry.input += usage.input;
        // The API counts thinking inside output tokens; the price table adds thinking to output, so keep them apart.
        entry.output += usage.output.saturating_sub(usage.thinking);
        entry.thinking += usage.thinking.min(usage.output);
        entry.cache_read += usage.cache_read;
        entry.cache_write_5m += usage.cache_write_5m;
        entry.cache_write_1h += usage.cache_write_1h;
    }
    by_model.into_values().collect()
}

/// Intervals in which this transcript's agent waited on an API reply or a tool.
/// Only replies first seen in this transcript count; a copy of another agent's reply is not this agent's work.
fn busy_intervals(transcript: &Transcript, owned: &HashSet<&str>) -> Vec<Interval> {
    let records = &transcript.records;
    let mut request_starts: BTreeMap<&str, Millis> = BTreeMap::new();
    let mut reply_spans: BTreeMap<&str, Interval> = BTreeMap::new();
    let mut tool_starts: BTreeMap<&str, Millis> = BTreeMap::new();
    let mut intervals = Vec::new();
    let mut previous: Option<Millis> = None;
    for record in records {
        let Some(at) = record.timestamp else { continue };
        match &record.kind {
            RecordKind::ApiRequest { id } => {
                request_starts.insert(id, at);
            }
            RecordKind::Assistant(line) if owned.contains(line.message_id.as_str()) => {
                let start = line
                    .request_ref
                    .as_deref()
                    .and_then(|r| request_starts.get(r).copied())
                    .or(previous)
                    .unwrap_or(at);
                let span = reply_spans
                    .entry(&line.message_id)
                    .or_insert(Interval::new(start, at));
                span.end = span.end.max(at);
                for tool in &line.tool_uses {
                    tool_starts.insert(&tool.id, at);
                }
            }
            RecordKind::ToolResults { tool_use_ids } => {
                for id in tool_use_ids {
                    if let Some(start) = tool_starts.remove(id.as_str()) {
                        intervals.push(Interval::new(start, at));
                    }
                }
            }
            _ => {}
        }
        previous = Some(at);
    }
    // A tool with no result was still running when the agent stopped.
    if let Some(last) = records.iter().filter_map(|r| r.timestamp).max() {
        intervals.extend(
            tool_starts
                .values()
                .map(|&start| Interval::new(start, last)),
        );
    }
    intervals.extend(reply_spans.into_values());
    intervals
}

fn agents(
    transcripts: &[Transcript],
    replies: &[(usize, &AssistantLine)],
    busy_by_agent: &[Vec<Interval>],
) -> Vec<Agent> {
    transcripts
        .iter()
        .enumerate()
        .map(|(index, transcript)| {
            let own: Vec<&AssistantLine> = replies
                .iter()
                .filter(|(i, r)| *i == index && r.model != SYNTHETIC_MODEL)
                .map(|(_, r)| *r)
                .collect();
            let tokens = own
                .iter()
                .map(|r| {
                    r.usage.input
                        + r.usage.output
                        + r.usage.cache_read
                        + r.usage.cache_write_5m
                        + r.usage.cache_write_1h
                })
                .sum();
            let (started, ended) = span(transcript);
            let info = &transcript.agent;
            Agent {
                id: info.id.clone(),
                parent: info.parent.clone(),
                agent_type: info.agent_type.clone(),
                model: info.model.clone().unwrap_or_else(|| most_used_model(&own)),
                depth: info.depth,
                tokens,
                busy_seconds: seconds(intervals::covered_millis(&busy_by_agent[index])),
                started: started.map(timestamp::format).unwrap_or_default(),
                ended: ended.map(timestamp::format).unwrap_or_default(),
            }
        })
        .collect()
}

fn most_used_model(replies: &[&AssistantLine]) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for reply in replies {
        *counts.entry(&reply.model).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(model, _)| model.to_string())
        .unwrap_or_default()
}

fn run_time(transcripts: &[Transcript], busy_by_agent: &[Vec<Interval>]) -> RunTime {
    let all_busy: Vec<Interval> = busy_by_agent.iter().flatten().copied().collect();
    let spans: Vec<(Option<Millis>, Option<Millis>)> = transcripts.iter().map(span).collect();
    let started = spans.iter().filter_map(|s| s.0).min();
    let ended = spans.iter().filter_map(|s| s.1).max();
    // A fold from 0.0, because the sum of no floats is -0.0 and would print as "-0h".
    let retry_millis: f64 = last_cost_states(transcripts)
        .map(|c| (c.api_millis - c.api_millis_without_retries).max(0.0))
        .fold(0.0, |total, millis| total + millis);
    RunTime {
        busy_seconds: seconds(intervals::covered_millis(&all_busy)),
        wall_seconds: match (started, ended) {
            (Some(s), Some(e)) => seconds(e - s),
            _ => 0.0,
        },
        agent_seconds: busy_by_agent
            .iter()
            .map(|b| seconds(intervals::covered_millis(b)))
            .sum(),
        rate_limit_seconds: retry_millis / 1000.0,
        human_wait_seconds: seconds(
            activity::operator_replies(transcripts)
                .iter()
                .map(|r| r.waited)
                .sum(),
        ),
        started: started.map(timestamp::format).unwrap_or_default(),
        ended: ended.map(timestamp::format).unwrap_or_default(),
    }
}

fn span(transcript: &Transcript) -> (Option<Millis>, Option<Millis>) {
    let times = transcript.records.iter().filter_map(|r| r.timestamp);
    (times.clone().min(), times.max())
}

/// The last cost record of each main session: Claude Code rewrites it as the session goes on.
fn last_cost_states(
    transcripts: &[Transcript],
) -> impl Iterator<Item = transcript::CostState> + '_ {
    transcripts
        .iter()
        .filter(|t| t.agent.is_main_session())
        .filter_map(|t| {
            t.records.iter().rev().find_map(|r| match r.kind {
                RecordKind::CostState(cost) => Some(cost),
                _ => None,
            })
        })
}

fn transcript_cost(transcripts: &[Transcript]) -> Option<f64> {
    let costs: Vec<f64> = last_cost_states(transcripts)
        .map(|c| c.total_cost_usd)
        .collect();
    (!costs.is_empty()).then(|| costs.iter().sum())
}

fn versions(transcripts: &[Transcript]) -> Vec<String> {
    let found: BTreeSet<&str> = transcripts
        .iter()
        .flat_map(|t| &t.records)
        .filter_map(|r| r.version.as_deref())
        .collect();
    found.into_iter().map(str::to_string).collect()
}

fn seconds(millis: Millis) -> f64 {
    millis as f64 / 1000.0
}

#[cfg(test)]
mod tests;
