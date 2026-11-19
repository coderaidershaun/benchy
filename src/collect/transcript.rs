//! Finds the transcripts of one Claude Code configuration directory and reads the parts benchy needs.
//! The format is internal to Claude Code, so every field we rely on is checked and a missing one is an error.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::CollectError;
use super::timestamp::{self, Millis};

/// Claude Code versions whose transcript shape this reader was checked against.
pub const OLDEST_KNOWN_VERSION: (u32, u32, u32) = (2, 1, 316);
pub const NEWEST_KNOWN_VERSION: (u32, u32, u32) = (2, 1, 327);

/// Model name Claude Code writes on the placeholder reply it records for a failed request.
pub const SYNTHETIC_MODEL: &str = "<synthetic>";

#[derive(Debug, Clone)]
pub struct Transcript {
    pub path: PathBuf,
    pub agent: AgentInfo,
    pub records: Vec<Record>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentInfo {
    pub id: String,
    pub session_id: String,
    pub parent: Option<String>,
    pub agent_type: String,
    pub model: Option<String>,
    pub depth: u32,
}

impl AgentInfo {
    pub fn is_main_session(&self) -> bool {
        self.depth == 0
    }
}

#[derive(Debug, Clone)]
pub struct Record {
    /// One-based line number in the transcript file.
    pub line: usize,
    pub timestamp: Option<Millis>,
    pub cwd: Option<String>,
    pub version: Option<String>,
    pub kind: RecordKind,
}

#[derive(Debug, Clone)]
pub enum RecordKind {
    Assistant(AssistantLine),
    ApiRequest { id: String },
    ToolResults { tool_use_ids: Vec<String> },
    Prompt { is_operator: bool },
    CostState(CostState),
    Other,
}

/// One line of an API reply. A reply with several content blocks is written as several lines with one message id.
#[derive(Debug, Clone)]
pub struct AssistantLine {
    pub message_id: String,
    pub model: String,
    pub usage: Usage,
    pub request_ref: Option<String>,
    pub tool_uses: Vec<ToolUse>,
    pub api_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    pub input: u64,
    /// Output tokens as the API reports them, thinking included.
    pub output: u64,
    pub thinking: u64,
    pub cache_read: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CostState {
    pub total_cost_usd: f64,
    pub api_millis: f64,
    pub api_millis_without_retries: f64,
}

/// Every transcript under `<config>/projects`, main sessions first.
///
/// # Errors
///
/// Fails on a file in a place this reader does not know, an unreadable file, or a record of unknown shape.
pub fn read_projects(projects: &Path) -> Result<Vec<Transcript>, CollectError> {
    let mut mains = Vec::new();
    let mut subagents = Vec::new();
    for project in sorted_entries(projects)? {
        if !project.is_dir() {
            continue;
        }
        for entry in sorted_entries(&project)? {
            let is_jsonl = entry.extension().is_some_and(|e| e == "jsonl");
            if entry.is_file() && is_jsonl {
                mains.push(read_main(&entry)?);
            } else if entry.is_dir() {
                subagents.extend(read_session_dir(&entry)?);
            }
        }
    }
    mains.extend(subagents);
    Ok(mains)
}

fn read_main(path: &Path) -> Result<Transcript, CollectError> {
    let session_id = file_stem(path);
    let agent = AgentInfo {
        id: session_id.clone(),
        session_id,
        parent: None,
        agent_type: "main".to_string(),
        model: None,
        depth: 0,
    };
    read_file(path, agent)
}

fn read_session_dir(dir: &Path) -> Result<Vec<Transcript>, CollectError> {
    let session_id = file_stem(dir);
    let mut transcripts = Vec::new();
    for child in sorted_entries(dir)? {
        let name = child.file_name().unwrap_or_default().to_string_lossy();
        if child.is_dir() && name == "subagents" {
            for file in sorted_entries(&child)? {
                if file.extension().is_some_and(|e| e == "jsonl") {
                    let agent = subagent_info(&file, &session_id)?;
                    transcripts.push(read_file(&file, agent)?);
                }
            }
        } else if child.extension().is_some_and(|e| e == "jsonl") {
            return Err(CollectError::Layout {
                path: child.display().to_string(),
            });
        }
    }
    Ok(transcripts)
}

fn subagent_info(file: &Path, session_id: &str) -> Result<AgentInfo, CollectError> {
    let stem = file_stem(file);
    let id = stem.strip_prefix("agent-").unwrap_or(&stem).to_string();
    let meta_path = file.with_file_name(format!("{stem}.meta.json"));
    let meta: Value = if meta_path.exists() {
        let text = read_text(&meta_path)?;
        serde_json::from_str(&text).map_err(|source| CollectError::Json {
            path: meta_path.display().to_string(),
            line: 1,
            source,
        })?
    } else {
        Value::Null
    };
    let text_of = |key: &str| meta.get(key).and_then(Value::as_str).map(str::to_string);
    let depth = meta
        .get("spawnDepth")
        .and_then(Value::as_u64)
        .and_then(|d| u32::try_from(d).ok())
        .unwrap_or(1);
    Ok(AgentInfo {
        id,
        session_id: session_id.to_string(),
        parent: Some(text_of("parentAgentId").unwrap_or_else(|| session_id.to_string())),
        agent_type: text_of("agentType").unwrap_or_else(|| "unknown".to_string()),
        model: text_of("model"),
        depth,
    })
}

fn read_file(path: &Path, agent: AgentInfo) -> Result<Transcript, CollectError> {
    let text = read_text(path)?;
    let shown = path.display().to_string();
    let lines: Vec<&str> = text.split('\n').collect();
    let mut records = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let line_number = index + 1;
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            // A process killed mid-write leaves a cut-off last line with no newline after it.
            Err(_) if index + 1 == lines.len() => continue,
            Err(source) => {
                return Err(CollectError::Json {
                    path: shown,
                    line: line_number,
                    source,
                });
            }
        };
        records.push(parse_record(&value, &shown, line_number)?);
    }
    Ok(Transcript {
        path: path.to_path_buf(),
        agent,
        records,
    })
}

fn parse_record(value: &Value, path: &str, line: usize) -> Result<Record, CollectError> {
    let version = value.get("version").and_then(Value::as_str);
    if let Some(version) = version {
        check_version(version, path, line)?;
    }
    let shape_error = |what: &str| CollectError::Shape {
        path: path.to_string(),
        line,
        version: version.unwrap_or("unknown").to_string(),
        what: what.to_string(),
    };
    let timestamp = match value.get("timestamp").and_then(Value::as_str) {
        Some(text) => Some(timestamp::parse(text).ok_or_else(|| shape_error("a timestamp"))?),
        None => None,
    };
    let kind = match value.get("type").and_then(Value::as_str) {
        Some("assistant") => RecordKind::Assistant(parse_assistant(value).map_err(shape_error)?),
        Some("api-request") => RecordKind::ApiRequest {
            id: text(value, "id").ok_or_else(|| shape_error("api-request.id"))?,
        },
        Some("user") => parse_user(value).ok_or_else(|| shape_error("user.message.content"))?,
        Some("cost-state") => RecordKind::CostState(parse_cost_state(value).map_err(shape_error)?),
        Some(_) => RecordKind::Other,
        None => return Err(shape_error("a record type")),
    };
    if timestamp.is_none()
        && matches!(
            kind,
            RecordKind::Assistant(_) | RecordKind::ToolResults { .. } | RecordKind::Prompt { .. }
        )
    {
        return Err(shape_error("a timestamp"));
    }
    Ok(Record {
        line,
        timestamp,
        cwd: text(value, "cwd"),
        version: version.map(str::to_string),
        kind,
    })
}

fn check_version(version: &str, path: &str, line: usize) -> Result<(), CollectError> {
    let parsed = parse_version(version);
    let is_known =
        parsed.is_some_and(|v| (OLDEST_KNOWN_VERSION..=NEWEST_KNOWN_VERSION).contains(&v));
    if is_known {
        Ok(())
    } else {
        Err(CollectError::UnknownVersion {
            path: path.to_string(),
            line,
            version: version.to_string(),
        })
    }
}

fn parse_version(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.split('.').map(|p| p.parse::<u32>().ok());
    let triple = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(triple)
}

pub fn version_text((major, minor, patch): (u32, u32, u32)) -> String {
    format!("{major}.{minor}.{patch}")
}

fn parse_assistant(value: &Value) -> Result<AssistantLine, &'static str> {
    let message = value.get("message").ok_or("assistant.message")?;
    let usage = message.get("usage").ok_or("message.usage")?;
    let count = |field: &'static str| usage.get(field).and_then(Value::as_u64).ok_or(field);
    let cache_creation = usage.get("cache_creation");
    let cache_count = |field: &str| {
        cache_creation
            .and_then(|c| c.get(field))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    let thinking = usage
        .get("output_tokens_details")
        .and_then(|d| d.get("thinking_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let usage = Usage {
        input: count("input_tokens")?,
        output: count("output_tokens")?,
        thinking,
        cache_read: count("cache_read_input_tokens")?,
        cache_write_5m: cache_count("ephemeral_5m_input_tokens"),
        cache_write_1h: cache_count("ephemeral_1h_input_tokens"),
    };
    let cache_written = count("cache_creation_input_tokens").unwrap_or(0);
    if cache_written != usage.cache_write_5m + usage.cache_write_1h {
        return Err("message.usage.cache_creation split into 5m and 1h");
    }
    let content = message
        .get("content")
        .and_then(Value::as_array)
        .ok_or("message.content")?;
    let mut tool_uses = Vec::new();
    for block in content {
        if block.get("type").and_then(Value::as_str) == Some("tool_use") {
            tool_uses.push(ToolUse {
                id: text(block, "id").ok_or("tool_use.id")?,
                name: text(block, "name").ok_or("tool_use.name")?,
                input: block.get("input").cloned().unwrap_or(Value::Null),
            });
        }
    }
    let is_api_error = value.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true);
    Ok(AssistantLine {
        message_id: text(message, "id").ok_or("message.id")?,
        model: text(message, "model").ok_or("message.model")?,
        usage,
        request_ref: text(value, "requestRef"),
        tool_uses,
        api_error: is_api_error
            .then(|| text(value, "error").unwrap_or_else(|| "unknown".to_string())),
    })
}

fn parse_user(value: &Value) -> Option<RecordKind> {
    let content = value.get("message")?.get("content")?;
    let blocks = match content {
        Value::String(_) => Vec::new(),
        Value::Array(blocks) => blocks.clone(),
        _ => return None,
    };
    let tool_use_ids: Vec<String> = blocks
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
        .filter_map(|b| text(b, "tool_use_id"))
        .collect();
    if !tool_use_ids.is_empty() {
        return Some(RecordKind::ToolResults { tool_use_ids });
    }
    let is_meta = value.get("isMeta").and_then(Value::as_bool) == Some(true);
    let origin = value
        .get("origin")
        .and_then(|o| o.get("kind"))
        .and_then(Value::as_str);
    let is_operator = !is_meta && matches!(origin, None | Some("human"));
    Some(RecordKind::Prompt { is_operator })
}

fn parse_cost_state(value: &Value) -> Result<CostState, &'static str> {
    let number = |field: &'static str| value.get(field).and_then(Value::as_f64).ok_or(field);
    let api_millis = number("totalAPIDuration")?;
    Ok(CostState {
        total_cost_usd: number("totalCostUSD")?,
        api_millis,
        api_millis_without_retries: number("totalAPIDurationWithoutRetries").unwrap_or(api_millis),
    })
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn read_text(path: &Path) -> Result<String, CollectError> {
    fs::read_to_string(path).map_err(|source| CollectError::Read {
        path: path.display().to_string(),
        source,
    })
}

fn sorted_entries(dir: &Path) -> Result<Vec<PathBuf>, CollectError> {
    let read_error = |source| CollectError::Read {
        path: dir.display().to_string(),
        source,
    };
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).map_err(read_error)? {
        entries.push(entry.map_err(read_error)?.path());
    }
    entries.sort();
    Ok(entries)
}
