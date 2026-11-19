//! Runs the harness headless: `claude -p` with the kickoff message, a cost cap and a time limit, in the run's environment.

use std::fs::File;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::RunError;
use super::config;
use super::new::CreatedRun;
use super::record::LaunchRecord;
use crate::collect::timestamp;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Caps {
    pub max_budget_usd: f64,
    pub time_limit: Duration,
}

/// Starts Claude Code, waits for it to exit or for the time limit, and records what happened in the run record.
///
/// # Errors
///
/// Fails when Claude Code cannot be started or its output files cannot be written.
pub fn headless(
    created: &mut CreatedRun,
    caps: Caps,
    program: &std::path::Path,
) -> Result<LaunchRecord, RunError> {
    let operator = &created.operator;
    let output_path = operator.claude_output();
    let stdout = create(&output_path)?;
    let stderr = create(&operator.dir.join("claude-stderr.txt"))?;
    let mut arguments = vec![
        "-p".to_string(),
        created.manifest.kickoff_message(),
        "--output-format".to_string(),
        "json".to_string(),
        "--max-budget-usd".to_string(),
        caps.max_budget_usd.to_string(),
    ];
    arguments.extend(config::claude_arguments(
        &created.manifest,
        &created.run.config(),
    ));
    let started = timestamp::now();
    // Its own process group, so the time limit stops the tools it started as well.
    let mut child = Command::new(program)
        .args(&arguments)
        .env_clear()
        .envs(&created.environment.variables)
        .current_dir(created.run.workspace())
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .process_group(0)
        .spawn()
        .map_err(|source| RunError::Io {
            action: format!("start {}", program.display()),
            source,
        })?;
    let deadline = Instant::now() + caps.time_limit;
    let mut hit_time_limit = false;
    let status = loop {
        let polled = child.try_wait().map_err(|source| RunError::Io {
            action: "wait for claude".to_string(),
            source,
        })?;
        if let Some(status) = polled {
            break Some(status);
        }
        if Instant::now() >= deadline {
            hit_time_limit = true;
            stop_group(child.id());
            break child.wait().ok();
        }
        thread::sleep(Duration::from_millis(500));
    };
    let result = std::fs::read_to_string(&output_path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let launch = LaunchRecord {
        started: timestamp::format(started),
        ended: timestamp::format(timestamp::now()),
        max_budget_usd: caps.max_budget_usd,
        time_limit_seconds: caps.time_limit.as_secs(),
        exit_code: status.and_then(|s| s.code()),
        hit_time_limit,
        result_subtype: result
            .as_ref()
            .and_then(|r| r.get("subtype"))
            .and_then(Value::as_str)
            .map(str::to_string),
        reported_cost_usd: result
            .as_ref()
            .and_then(|r| r.get("total_cost_usd"))
            .and_then(Value::as_f64),
        error: result.as_ref().and_then(error_message),
    };
    created.record.launch = Some(launch.clone());
    created.record.save(&operator.record())?;
    Ok(launch)
}

fn error_message(result: &Value) -> Option<String> {
    if result.get("is_error").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let text = result
        .get("result")
        .and_then(Value::as_str)
        .unwrap_or("no message");
    let reason = result
        .get("terminal_reason")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    Some(format!("{text} ({reason})"))
}

/// Sends TERM to the whole group, then KILL to whatever is left a few seconds later.
fn stop_group(pid: u32) {
    let group = format!("-{pid}");
    for signal in ["-TERM", "-KILL"] {
        // A failure here means the group is already gone, which is the goal.
        let _ = Command::new("kill").args([signal, "--", &group]).status();
        thread::sleep(Duration::from_secs(5));
    }
}

fn create(path: &std::path::Path) -> Result<File, RunError> {
    File::create(path).map_err(|source| RunError::Io {
        action: format!("create {}", path.display()),
        source,
    })
}
