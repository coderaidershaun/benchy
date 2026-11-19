//! The audit of tool calls: flags every call that reaches outside the run directory, for the operator to review.
//! It is a second alarm. The run's own configuration directory and neutral path are what keep benchy out of reach.

mod paths;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::collect::transcript::{RecordKind, Transcript};

pub use paths::normalise;

/// Where the run may work, and what counts as benchy material.
#[derive(Debug, Clone)]
pub struct AuditScope {
    /// `<run root>/<run-id>`: the workspace and the run's configuration.
    pub run_dir: PathBuf,
    pub home: PathBuf,
    /// The benchy checkout. A path into it is the most serious finding.
    pub benchy_root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FindingKind {
    /// A path into the benchy checkout, or the word benchy: a confirmed read voids the run.
    Benchy,
    /// A path outside the run directory that is not a system or toolchain location.
    OutsideRun,
    /// A search that starts at the root of the disk or the home directory.
    DiskSearch,
    /// A clone, download or web tool: allowed, but shown so the operator can see what was fetched.
    Network,
    /// The agent's working directory was outside the run directory.
    WorkingDirectory,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub kind: FindingKind,
    pub file: String,
    pub line: usize,
    pub tool: String,
    /// The path or command that raised the finding, cut to a readable length.
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AuditReport {
    pub tool_calls_checked: u32,
    pub findings: Vec<Finding>,
}

impl AuditReport {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    pub fn names_benchy(&self) -> bool {
        self.findings.iter().any(|f| f.kind == FindingKind::Benchy)
    }
}

/// Read-only system and toolchain locations that a build legitimately touches.
const ALLOWED_ROOTS: [&str; 12] = [
    "/usr",
    "/bin",
    "/sbin",
    "/opt/homebrew",
    "/Library/Developer",
    "/System",
    "/Applications/Xcode.app",
    "/dev/null",
    "/dev/stdin",
    "/dev/stdout",
    "/dev/stderr",
    "/etc/hosts",
];
const ALLOWED_IN_HOME: [&str; 6] = [
    ".cargo",
    ".rustup",
    "go",
    ".nuget",
    ".dotnet",
    "Library/Caches",
];
const SEARCH_PROGRAMS: [&str; 7] = ["find", "grep", "rg", "fd", "ag", "locate", "mdfind"];
const NETWORK_WORDS: [&str; 5] = ["git clone", "gh repo clone", "curl ", "wget ", "git fetch"];
const NETWORK_TOOLS: [&str; 2] = ["WebFetch", "WebSearch"];

pub fn audit(transcripts: &[Transcript], scope: &AuditScope) -> AuditReport {
    let mut report = AuditReport::default();
    for transcript in transcripts {
        let file = transcript.path.display().to_string();
        for record in &transcript.records {
            let cwd = record.cwd.as_deref().map(PathBuf::from);
            if let Some(cwd) = &cwd
                && !scope.is_inside_run(cwd)
            {
                report.add(
                    FindingKind::WorkingDirectory,
                    &file,
                    record.line,
                    "-",
                    &cwd.display().to_string(),
                );
            }
            let RecordKind::Assistant(line) = &record.kind else {
                continue;
            };
            for tool in &line.tool_uses {
                report.tool_calls_checked += 1;
                let base = cwd.clone().unwrap_or_else(|| scope.run_dir.join("ws"));
                for (kind, evidence) in scope.check_call(&tool.name, &tool.input, &base) {
                    report.add(kind, &file, record.line, &tool.name, &evidence);
                }
            }
        }
    }
    report
}

impl AuditReport {
    fn add(&mut self, kind: FindingKind, file: &str, line: usize, tool: &str, evidence: &str) {
        let evidence: String = evidence.chars().take(200).collect();
        let finding = Finding {
            kind,
            file: file.to_string(),
            line,
            tool: tool.to_string(),
            evidence,
        };
        if !self.findings.contains(&finding) {
            self.findings.push(finding);
        }
    }
}

impl AuditScope {
    fn is_inside_run(&self, path: &Path) -> bool {
        normalise(path).starts_with(normalise(&self.run_dir))
    }

    fn is_allowed(&self, path: &Path) -> bool {
        self.is_inside_run(path)
            || ALLOWED_ROOTS.iter().any(|root| path.starts_with(root))
            || ALLOWED_IN_HOME
                .iter()
                .any(|dir| path.starts_with(self.home.join(dir)))
    }

    fn check_call(&self, tool: &str, input: &Value, cwd: &Path) -> Vec<(FindingKind, String)> {
        let mut findings = Vec::new();
        if NETWORK_TOOLS.contains(&tool) {
            findings.push((FindingKind::Network, input.to_string()));
        }
        for text in strings_in(input) {
            if text.to_ascii_lowercase().contains("benchy") {
                findings.push((FindingKind::Benchy, text.clone()));
            }
            if NETWORK_WORDS.iter().any(|w| text.contains(w)) {
                findings.push((FindingKind::Network, text.clone()));
            }
            if is_disk_search(&text) {
                findings.push((FindingKind::DiskSearch, text.clone()));
            }
            for candidate in paths::candidates(&text) {
                let resolved = paths::resolve(&candidate, cwd, &self.home);
                if resolved.starts_with(normalise(&self.benchy_root)) {
                    findings.push((FindingKind::Benchy, candidate));
                } else if !self.is_allowed(&resolved) {
                    findings.push((FindingKind::OutsideRun, candidate));
                }
            }
        }
        findings
    }
}

/// `find / ...`, `grep -r ... ~`, `rg pattern $HOME` and the like.
fn is_disk_search(command: &str) -> bool {
    let words: Vec<&str> = command.split_whitespace().collect();
    let starts_search = |w: &&str| SEARCH_PROGRAMS.contains(&w.rsplit('/').next().unwrap_or(w));
    let wide_roots = [
        "/", "~", "~/", "$HOME", "${HOME}", "/Users", "/home", "/private", "/Volumes",
    ];
    words.iter().any(starts_search)
        && words
            .iter()
            .any(|w| wide_roots.contains(&w.trim_matches(|c| c == '"' || c == '\'')))
}

fn strings_in(value: &Value) -> Vec<String> {
    match value {
        Value::String(text) => vec![text.clone()],
        Value::Array(items) => items.iter().flat_map(strings_in).collect(),
        Value::Object(fields) => fields.values().flat_map(strings_in).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
