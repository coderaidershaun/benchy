//! Finds processes that still belong to a run, so `finish` never snapshots a workspace that is still changing.
//! A process belongs to the run when its working directory or its command line is inside the run directory.

use std::path::Path;
use std::process::Command;

use super::RunError;
use super::layout::canonical_or_given;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveProcess {
    pub pid: u32,
    pub command: String,
}

/// # Errors
///
/// Fails when neither `ps` nor `lsof` can be run, since then no process could be ruled out.
pub fn alive_in(run_dir: &Path) -> Result<Vec<LiveProcess>, RunError> {
    let real = canonical_or_given(run_dir).display().to_string();
    let given = run_dir.display().to_string();
    let own = std::process::id();
    let commands = list("ps", &["-axww", "-o", "pid=,command="])?;
    let mut alive: Vec<LiveProcess> = commands
        .lines()
        .filter_map(|line| {
            let (pid, command) = line.trim_start().split_once(' ')?;
            Some(LiveProcess {
                pid: pid.parse().ok()?,
                command: command.trim().to_string(),
            })
        })
        .filter(|p| p.pid != own && (p.command.contains(&real) || p.command.contains(&given)))
        .collect();
    for pid in pids_with_cwd_inside(&real)? {
        if pid != own && !alive.iter().any(|p| p.pid == pid) {
            let command = commands
                .lines()
                .find_map(|l| {
                    l.trim_start()
                        .strip_prefix(&format!("{pid} "))
                        .map(str::to_string)
                })
                .unwrap_or_default();
            alive.push(LiveProcess { pid, command });
        }
    }
    Ok(alive)
}

/// `lsof -F pn` prints one `p<pid>` line, then one `n<path>` line for the working directory.
fn pids_with_cwd_inside(dir: &str) -> Result<Vec<u32>, RunError> {
    let output = list("lsof", &["-w", "-d", "cwd", "-F", "pn"])?;
    let mut pids = Vec::new();
    let mut current: Option<u32> = None;
    for line in output.lines() {
        if let Some(pid) = line.strip_prefix('p') {
            current = pid.parse().ok();
        } else if let (Some(path), Some(pid)) = (line.strip_prefix('n'), current)
            && Path::new(path).starts_with(dir)
        {
            pids.push(pid);
        }
    }
    Ok(pids)
}

fn list(program: &str, arguments: &[&str]) -> Result<String, RunError> {
    let output = Command::new(program)
        .args(arguments)
        .output()
        .map_err(|source| RunError::Io {
            action: format!("run {program} to look for processes of the run"),
            source,
        })?;
    // lsof exits 1 when some processes could not be read; what it could read is still printed.
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
