//! Checks the host before a run: Claude Code, the toolchains, Docker, the run root and the files Claude Code would read from it.

use std::path::Path;
use std::process::Command;

use benchy::contract::{AuthMode, HarnessManifest, Toolchains};

use super::RunError;
use super::layout;
use crate::collect::transcript::{NEWEST_KNOWN_VERSION, OLDEST_KNOWN_VERSION, version_text};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub verdict: Verdict,
    pub name: &'static str,
    pub detail: String,
}

impl std::fmt::Display for Check {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let word = match self.verdict {
            Verdict::Ok => "ok  ",
            Verdict::Warn => "warn",
            Verdict::Fail => "FAIL",
        };
        write!(f, "{word} {:<14} {}", self.name, self.detail)
    }
}

/// Files that Claude Code reads from a working directory and every directory above it.
const INHERITED_FILES: [&str; 3] = ["CLAUDE.md", "CLAUDE.local.md", ".claude/CLAUDE.md"];

/// # Errors
///
/// Fails when the pinned toolchains or the manifest cannot be read.
pub fn doctor(
    benchy_root: &Path,
    run_root: &Path,
    manifest: Option<&Path>,
) -> Result<Vec<Check>, RunError> {
    let toolchains = Toolchains::load(&benchy_root.join("validation/toolchains.toml"))?;
    let manifest = manifest.map(HarnessManifest::load).transpose()?;
    let mut checks = vec![claude_check(manifest.as_ref())];
    checks.push(version_check(
        "rust",
        &["rustc", "--version"],
        &toolchains.rust.version,
    ));
    checks.push(version_check(
        "go",
        &["go", "version"],
        &toolchains.go.version,
    ));
    checks.push(version_check(
        "dotnet",
        &["dotnet", "--version"],
        &toolchains.dotnet.version,
    ));
    checks.push(match output_of(&["docker", "--version"]) {
        Some(text) => ok("docker", text),
        None => fail("docker", "not found; the scorer runs in Docker".to_string()),
    });
    checks.push(match layout::check_apart(run_root, benchy_root) {
        Ok(()) => ok("run root", run_root.display().to_string()),
        Err(error) => fail("run root", error.to_string()),
    });
    checks.extend(inherited_files(run_root));
    if let Some(manifest) = &manifest {
        let secret = match manifest.auth {
            AuthMode::ApiKey => "ANTHROPIC_API_KEY",
            AuthMode::Subscription => super::environment::OAUTH_TOKEN,
        };
        checks.push(match std::env::var_os(secret) {
            Some(_) => ok("auth", format!("{secret} is set")),
            None => fail("auth", super::new::login_warning(manifest.auth)),
        });
    }
    Ok(checks)
}

fn claude_check(manifest: Option<&HarnessManifest>) -> Check {
    let Some(text) = output_of(&["claude", "--version"]) else {
        return fail("claude code", "claude is not on PATH".to_string());
    };
    let found = text
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();
    let known = format!(
        "{} to {}",
        version_text(OLDEST_KNOWN_VERSION),
        version_text(NEWEST_KNOWN_VERSION)
    );
    match manifest {
        Some(m) if m.claude_code.version != found => fail(
            "claude code",
            format!(
                "{found} is installed, the manifest declares {}",
                m.claude_code.version
            ),
        ),
        _ => ok(
            "claude code",
            format!("{found} (transcript shape known for {known})"),
        ),
    }
}

fn version_check(name: &'static str, command: &[&str], pinned: &str) -> Check {
    match output_of(command) {
        Some(text) if text.contains(pinned) => ok(name, text),
        Some(text) => warn(
            name,
            format!(
                "{text}; pinned {pinned}. The scorer image has the pinned version; a host run needs it too"
            ),
        ),
        None => warn(
            name,
            format!("not found; pinned {pinned}. A host run needs it, the scorer image has it"),
        ),
    }
}

fn inherited_files(run_root: &Path) -> Vec<Check> {
    let mut checks = Vec::new();
    for dir in layout::canonical_or_given(run_root).ancestors() {
        for name in INHERITED_FILES {
            let path = dir.join(name);
            if path.is_file() {
                checks.push(fail(
                    "inherited",
                    format!(
                        "{} would be read by every run; move it or choose another run root",
                        path.display()
                    ),
                ));
            }
        }
    }
    if checks.is_empty() {
        checks.push(ok(
            "inherited",
            "no CLAUDE.md above the run root".to_string(),
        ));
    }
    checks
}

fn output_of(command: &[&str]) -> Option<String> {
    let output = Command::new(command[0]).args(&command[1..]).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn ok(name: &'static str, detail: String) -> Check {
    Check {
        verdict: Verdict::Ok,
        name,
        detail,
    }
}

fn warn(name: &'static str, detail: String) -> Check {
    Check {
        verdict: Verdict::Warn,
        name,
        detail,
    }
}

fn fail(name: &'static str, detail: String) -> Check {
    Check {
        verdict: Verdict::Fail,
        name,
        detail,
    }
}
