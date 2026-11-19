//! Where a run lives: the run directory outside the repo, and the operator's data under `.runs/` of the benchy checkout.
//! The run directory holds only `ws` and `config`, under a random id that carries no label.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::RunError;

/// The environment variable that moves the run root away from `~/runs`, for tests and sandboxed runs.
pub const RUN_ROOT_VARIABLE: &str = "BENCHY_RUN_ROOT";

/// A random run id: 12 lowercase hex digits, so nothing about the harness shows in the path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunId(String);

impl RunId {
    /// # Errors
    ///
    /// Fails when the system's random source cannot be read.
    pub fn random() -> Result<RunId, RunError> {
        let mut bytes = [0u8; 6];
        fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut bytes))
            .map_err(|source| RunError::Io {
                action: "read random bytes from /dev/urandom".to_string(),
                source,
            })?;
        Ok(RunId(bytes.iter().map(|b| format!("{b:02x}")).collect()))
    }

    /// # Errors
    ///
    /// Fails on anything but lowercase hex digits, so an id can never climb out of its directory.
    pub fn parse(text: &str) -> Result<RunId, RunError> {
        let is_hex = !text.is_empty()
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if is_hex {
            Ok(RunId(text.to_string()))
        } else {
            Err(RunError::BadRunId {
                id: text.to_string(),
            })
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The run's own directory: `<run root>/<run-id>/ws` and `<run root>/<run-id>/config`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunDir {
    pub dir: PathBuf,
}

impl RunDir {
    pub fn new(root: &Path, id: &RunId) -> RunDir {
        RunDir {
            dir: root.join(id.as_str()),
        }
    }

    pub fn workspace(&self) -> PathBuf {
        self.dir.join("ws")
    }

    pub fn config(&self) -> PathBuf {
        self.dir.join("config")
    }

    pub fn projects(&self) -> PathBuf {
        self.config().join("projects")
    }
}

/// The operator's data of one run: `<benchy>/.runs/<run-id>/`. Never reachable from the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorDir {
    pub dir: PathBuf,
}

impl OperatorDir {
    pub fn new(benchy_root: &Path, id: &RunId) -> OperatorDir {
        OperatorDir {
            dir: benchy_root.join(".runs").join(id.as_str()),
        }
    }

    pub fn record(&self) -> PathBuf {
        self.dir.join("run.json")
    }

    pub fn manifest(&self) -> PathBuf {
        self.dir.join("harness.toml")
    }

    pub fn telemetry(&self) -> PathBuf {
        self.dir.join("telemetry.json")
    }

    pub fn audit(&self) -> PathBuf {
        self.dir.join("audit.json")
    }

    pub fn transcripts(&self) -> PathBuf {
        self.dir.join("transcripts")
    }

    pub fn workspace_tarball(&self) -> PathBuf {
        self.dir.join("workspace.tar")
    }

    pub fn claude_output(&self) -> PathBuf {
        self.dir.join("claude-output.json")
    }

    pub fn manual_entry(&self) -> PathBuf {
        self.dir.join("manual.toml")
    }
}

/// The run root from the flag, else from [`RUN_ROOT_VARIABLE`], else `~/runs`.
///
/// # Errors
///
/// Fails when no root is given and the home directory is unknown.
pub fn run_root(flag: Option<&Path>) -> Result<PathBuf, RunError> {
    if let Some(root) = flag {
        return Ok(absolute(root));
    }
    if let Some(root) = std::env::var_os(RUN_ROOT_VARIABLE) {
        return Ok(absolute(Path::new(&root)));
    }
    Ok(home()?.join("runs"))
}

/// The benchy checkout: the nearest directory at or above the current one that holds the task pack and the prices.
///
/// # Errors
///
/// Fails when no such directory exists above the current directory.
pub fn benchy_root() -> Result<PathBuf, RunError> {
    let cwd = std::env::current_dir().map_err(|source| RunError::Io {
        action: "read the current directory".to_string(),
        source,
    })?;
    cwd.ancestors()
        .find(|dir| {
            dir.join("task/TASK.md").is_file() && dir.join("validation/prices.toml").is_file()
        })
        .map(Path::to_path_buf)
        .ok_or_else(|| RunError::NotInBenchy {
            cwd: cwd.display().to_string(),
        })
}

/// # Errors
///
/// Fails when `HOME` is not set.
pub fn home() -> Result<PathBuf, RunError> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(RunError::NoHome)
}

/// Refuses a run root that shares a directory tree with the benchy checkout, which would put benchy in reach of the run.
///
/// # Errors
///
/// Fails when either directory holds the other.
pub fn check_apart(run_root: &Path, benchy_root: &Path) -> Result<(), RunError> {
    let run_root = canonical_or_given(run_root);
    let benchy_root = canonical_or_given(benchy_root);
    if run_root.starts_with(&benchy_root) || benchy_root.starts_with(&run_root) {
        return Err(RunError::RootNotApart {
            run_root: run_root.display().to_string(),
            benchy_root: benchy_root.display().to_string(),
        });
    }
    Ok(())
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The real path, with links resolved, of the deepest part of `path` that exists.
pub fn canonical_or_given(path: &Path) -> PathBuf {
    let mut missing = Vec::new();
    let mut existing = path.to_path_buf();
    loop {
        if let Ok(real) = existing.canonicalize() {
            return missing.iter().rev().fold(real, |acc, part| acc.join(part));
        }
        match (
            existing.file_name().map(ToOwned::to_owned),
            existing.parent(),
        ) {
            (Some(name), Some(parent)) => {
                missing.push(name);
                existing = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
}
