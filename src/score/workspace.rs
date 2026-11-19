//! Turns a workspace into the tree that is scored: its committed files, with the pristine `Makefile` and `lint/` of the task pack.
//! It also assembles the reference workspace from the task pack and the three reference solutions.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::ScoreError;
use crate::run::workspace::{DEFAULT_BRANCH, copy_tree};
use benchy::contract::Language;

/// The tree that scoring builds and measures, and the hash of what it holds.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub root: PathBuf,
    /// The commit of the default branch, or a hash of the files when the workspace has no commit.
    pub hash: String,
}

/// Copies the committed state of `workspace` into `into`, or all of its files when it has no
/// commit on the default branch, then puts the pristine `Makefile` and `lint/` of `task_pack` in place.
///
/// # Errors
///
/// Fails when files cannot be copied or git cannot export the commit.
pub fn prepare(workspace: &Path, task_pack: &Path, into: &Path) -> Result<Prepared, ScoreError> {
    fs::create_dir_all(into).map_err(ScoreError::io(format!("create {}", into.display())))?;
    let hash = match default_branch_head(workspace) {
        Some(head) => {
            let tarball = into.with_extension("tar");
            git(
                workspace,
                &[
                    "archive",
                    "--format=tar",
                    "--output",
                    &tarball.display().to_string(),
                    &head,
                ],
            )?;
            let status = Command::new("tar")
                .arg("-xf")
                .arg(&tarball)
                .arg("-C")
                .arg(into)
                .status()
                .map_err(ScoreError::io("run tar".to_string()))?;
            if !status.success() {
                return Err(ScoreError::Command(format!(
                    "tar -xf {} exited with {status}",
                    tarball.display()
                )));
            }
            drop(fs::remove_file(&tarball));
            head
        }
        None => {
            copy_tree(workspace, into).map_err(|error| ScoreError::Command(error.to_string()))?;
            drop(fs::remove_dir_all(into.join(".git")));
            format!("files:{:016x}", tree_hash(into))
        }
    };
    put_pristine(task_pack, into)?;
    Ok(Prepared {
        root: into.to_path_buf(),
        hash,
    })
}

fn put_pristine(task_pack: &Path, root: &Path) -> Result<(), ScoreError> {
    let lint = root.join("lint");
    if lint.exists() {
        fs::remove_dir_all(&lint).map_err(ScoreError::io(format!("remove {}", lint.display())))?;
    }
    copy_tree(&task_pack.join("lint"), &lint)
        .map_err(|error| ScoreError::Command(error.to_string()))?;
    fs::copy(task_pack.join("Makefile"), root.join("Makefile")).map_err(ScoreError::io(
        format!("copy the pristine Makefile into {}", root.display()),
    ))?;
    Ok(())
}

/// The commit at the head of the default branch, when `workspace` is a git repository that has one.
pub fn default_branch_head(workspace: &Path) -> Option<String> {
    if !workspace.join(".git").exists() {
        return None;
    }
    let reference = format!("refs/heads/{DEFAULT_BRANCH}");
    git(workspace, &["rev-parse", "--verify", "--quiet", &reference])
        .ok()
        .map(|out| out.trim().to_string())
        .filter(|head| !head.is_empty())
}

/// Runs git in `dir` with none of the caller's git settings, and returns its standard output.
///
/// # Errors
///
/// Fails when git cannot start or exits with an error.
pub fn git(dir: &Path, arguments: &[&str]) -> Result<String, ScoreError> {
    let output = Command::new("git")
        .args([
            "-c",
            "safe.directory=*",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
        ])
        .args([
            "-c",
            "user.name=scorer",
            "-c",
            "user.email=scorer@localhost",
        ])
        .args(arguments)
        .current_dir(dir)
        .env_clear()
        .envs(
            std::env::vars()
                .filter(|(name, _)| ["PATH", "HOME", "LANG", "TMPDIR"].contains(&name.as_str())),
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .map_err(ScoreError::io(format!("start git {}", arguments.join(" "))))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(ScoreError::Command(format!(
            "git {} in {}: {}",
            arguments.join(" "),
            dir.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// An FNV-1a hash of every file path and its bytes, in path order.
fn tree_hash(root: &Path) -> u64 {
    let mut files = Vec::new();
    list_files(root, root, &mut files);
    files.sort();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for relative in files {
        feed(relative.to_string_lossy().as_bytes());
        feed(&fs::read(root.join(&relative)).unwrap_or_default());
    }
    hash
}

fn list_files(root: &Path, dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            list_files(root, &path, found);
        } else if let Ok(relative) = path.strip_prefix(root) {
            found.push(relative.to_path_buf());
        }
    }
}

/// The `DELIVERY.md` of the reference workspace: how to build and test each language.
const REFERENCE_DELIVERY: &str = "# Delivery

The three reference engines of benchy, assembled from `validation/reference/`.

```sh
make build-rust test-rust
make build-go test-go
make build-csharp test-csharp
```
";

/// Builds the reference workspace in `into`: the task pack with each reference solution in its
/// language folder, committed on the default branch like a finished run.
///
/// # Errors
///
/// Fails when files cannot be copied or git fails.
pub fn assemble_reference(benchy_root: &Path, into: &Path) -> Result<(), ScoreError> {
    if into.exists() {
        fs::remove_dir_all(into).map_err(ScoreError::io(format!("remove {}", into.display())))?;
    }
    crate::run::workspace::create(&benchy_root.join("task"), into)
        .map_err(|error| ScoreError::Command(error.to_string()))?;
    for language in Language::ALL {
        let target = into.join(language.as_str());
        drop(fs::remove_file(target.join(".gitkeep")));
        let source = benchy_root
            .join("validation/reference")
            .join(language.as_str());
        copy_sources(&source, &target)?;
    }
    fs::write(into.join("DELIVERY.md"), REFERENCE_DELIVERY).map_err(ScoreError::io(
        "write the reference DELIVERY.md".to_string(),
    ))?;
    git(into, &["add", "--all"])?;
    git(into, &["commit", "--quiet", "-m", "Reference solutions"])?;
    Ok(())
}

/// Copies a reference solution without the build output a local build may have left in it.
fn copy_sources(from: &Path, to: &Path) -> Result<(), ScoreError> {
    fs::create_dir_all(to).map_err(ScoreError::io(format!("create {}", to.display())))?;
    let entries = fs::read_dir(from).map_err(ScoreError::io(format!("read {}", from.display())))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if ["target", "bin", "obj"].contains(&name.to_string_lossy().as_ref()) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            copy_sources(&path, &to.join(&name))?;
        } else {
            fs::copy(&path, to.join(&name))
                .map_err(ScoreError::io(format!("copy {}", path.display())))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_workspace_without_git_is_copied_and_gets_the_pristine_makefile() {
        let task = tempfile::tempdir().expect("task dir");
        fs::write(task.path().join("Makefile"), "pristine\n").expect("write");
        fs::create_dir_all(task.path().join("lint/rust")).expect("create lint");
        fs::write(task.path().join("lint/rust/clippy.txt"), "-Dwarnings\n").expect("write");
        let workspace = tempfile::tempdir().expect("workspace");
        fs::write(workspace.path().join("Makefile"), "edited\n").expect("write");
        fs::create_dir_all(workspace.path().join("lint/rust")).expect("create lint");
        fs::write(workspace.path().join("lint/rust/extra.txt"), "x\n").expect("write");
        let into = tempfile::tempdir().expect("into");

        let prepared =
            prepare(workspace.path(), task.path(), &into.path().join("tree")).expect("prepare");

        let makefile = fs::read_to_string(prepared.root.join("Makefile")).expect("read");
        assert_eq!(makefile, "pristine\n");
        assert!(!prepared.root.join("lint/rust/extra.txt").exists());
        assert!(prepared.hash.starts_with("files:"));
    }
}
