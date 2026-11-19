//! The workspace: a fresh git repository holding a verbatim copy of the task pack, and its snapshot when the run ends.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use super::RunError;

/// A neutral author, so the workspace's history names neither benchy nor the operator.
const GIT_IDENTITY: [&str; 4] = [
    "-c",
    "user.name=runner",
    "-c",
    "user.email=runner@localhost",
];
/// Settings that keep the operator's git hooks and signing out of the initial commit.
const GIT_QUIET: [&str; 6] = [
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "commit.gpgsign=false",
    "-c",
    "init.defaultBranch=main",
];

pub const DEFAULT_BRANCH: &str = "main";

/// Copies the task pack into `workspace` and commits it as the first commit of branch `main`.
///
/// # Errors
///
/// Fails when a file cannot be copied or git fails.
pub fn create(task_pack: &Path, workspace: &Path) -> Result<(), RunError> {
    copy_tree(task_pack, workspace)?;
    git(workspace, &["init", "--quiet"])?;
    git(workspace, &["add", "--all"])?;
    git(workspace, &["commit", "--quiet", "--message", "Task pack"])?;
    Ok(())
}

/// The committed state of the workspace when the run ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub head: Option<String>,
    pub branch: String,
    pub uncommitted: Vec<String>,
}

/// Records the head of the default branch and writes it as a tar file; uncommitted work is listed, not kept.
///
/// # Errors
///
/// Fails when git fails.
pub fn snapshot(workspace: &Path, tarball: &Path) -> Result<Snapshot, RunError> {
    let reference = format!("refs/heads/{DEFAULT_BRANCH}");
    let head = git(workspace, &["rev-parse", "--verify", "--quiet", &reference])
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|h| !h.is_empty());
    if let Some(head) = &head {
        let tarball = tarball.display().to_string();
        git(
            workspace,
            &["archive", "--format=tar", "--output", &tarball, head],
        )?;
    }
    let status = git(
        workspace,
        &["status", "--porcelain", "--untracked-files=all"],
    )?;
    let uncommitted = String::from_utf8_lossy(&status.stdout)
        .lines()
        .map(str::to_string)
        .collect();
    Ok(Snapshot {
        head,
        branch: DEFAULT_BRANCH.to_string(),
        uncommitted,
    })
}

fn git(workspace: &Path, arguments: &[&str]) -> Result<Output, RunError> {
    let shown = format!("git {}", arguments.join(" "));
    let output = Command::new("git")
        .args(GIT_IDENTITY)
        .args(GIT_QUIET)
        .args(arguments)
        .current_dir(workspace)
        .env_clear()
        .envs(git_environment(std::env::vars()))
        .output()
        .map_err(|source| RunError::Io {
            action: format!("start {shown}"),
            source,
        })?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(RunError::CommandFailed {
            command: format!(
                "{shown} in {}: {}",
                workspace.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            status: output.status.to_string(),
        })
    }
}

/// The only variables workspace git sees. A git hook that runs benchy sets `GIT_DIR` to the benchy repository,
/// and an inherited one would make every git command here act on benchy instead of the workspace.
fn git_environment(caller: impl Iterator<Item = (String, String)>) -> Vec<(String, String)> {
    let mut kept: Vec<(String, String)> = caller
        .filter(|(name, _)| ["PATH", "HOME", "LANG", "TMPDIR"].contains(&name.as_str()))
        .collect();
    kept.push(("GIT_CONFIG_NOSYSTEM".to_string(), "1".to_string()));
    kept.push(("GIT_CONFIG_GLOBAL".to_string(), "/dev/null".to_string()));
    kept
}

/// Copies every file and directory, dotfiles included. Links are refused so nothing points back out of the workspace.
pub fn copy_tree(from: &Path, to: &Path) -> Result<(), RunError> {
    let io_error = |action: String| move |source| RunError::Io { action, source };
    fs::create_dir_all(to).map_err(io_error(format!("create directory {}", to.display())))?;
    let entries =
        fs::read_dir(from).map_err(io_error(format!("read directory {}", from.display())))?;
    for entry in entries {
        let entry = entry.map_err(io_error(format!("read directory {}", from.display())))?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        let kind = entry
            .file_type()
            .map_err(io_error(format!("read {}", source.display())))?;
        if kind.is_dir() {
            copy_tree(&source, &target)?;
        } else if kind.is_file() {
            fs::copy(&source, &target).map_err(io_error(format!(
                "copy {} to {}",
                source.display(),
                target.display()
            )))?;
        } else {
            return Err(RunError::NotAPlainFile {
                path: source.display().to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_variables_of_the_calling_process_never_reach_the_workspace_git() {
        // A git hook that runs the tests sets GIT_DIR to the benchy repository.
        let caller = [
            ("GIT_DIR", "/benchy/.git"),
            ("GIT_INDEX_FILE", "/benchy/.git/index"),
            ("GIT_WORK_TREE", "/benchy"),
            ("PATH", "/usr/bin"),
            ("HOME", "/home/u"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()));
        let kept: Vec<String> = git_environment(caller.into_iter())
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert_eq!(
            kept,
            ["PATH", "HOME", "GIT_CONFIG_NOSYSTEM", "GIT_CONFIG_GLOBAL"]
        );
    }
}
