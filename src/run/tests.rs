//! Lifecycle tests on scratch directories: a new run is isolated, and finish snapshots it, refuses while it is busy,
//! and falls back to a manual entry when there are no transcripts.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use benchy::contract::TelemetrySource;

use super::RunError;
use super::finish::Finish;
use super::new::{CreatedRun, NewRun};

const TOY_MANIFEST: &str = r#"
schema = 1
label = "toy"
description = "A toy harness for tests."
entry = "{prompt}"
launch = "interactive"
permission_mode = "bypassPermissions"
auth = "subscription"
isolation = "clean-config"

[claude_code]
version = "2.1.327"
model = "claude-haiku-5-5"
"#;

struct Scratch {
    _dir: tempfile::TempDir,
    benchy_root: PathBuf,
    run_root: PathBuf,
    manifest: PathBuf,
}

fn scratch() -> Scratch {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let benchy_root = dir.path().join("checkout");
    let run_root = dir.path().join("runs");
    fs::create_dir_all(benchy_root.join("validation")).expect("the fake checkout");
    fs::copy(
        repo().join("validation/prices.toml"),
        benchy_root.join("validation/prices.toml"),
    )
    .expect("prices");
    let manifest = benchy_root.join("toy.toml");
    fs::write(&manifest, TOY_MANIFEST).expect("the manifest");
    Scratch {
        _dir: dir,
        benchy_root,
        run_root,
        manifest,
    }
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn create(scratch: &Scratch) -> CreatedRun {
    NewRun {
        manifest_path: scratch.manifest.clone(),
        benchy_root: scratch.benchy_root.clone(),
        run_root: scratch.run_root.clone(),
        task_pack: Some(repo().join("task")),
    }
    .create()
    .expect("the run is created")
}

fn mentions(dir: &Path, needle: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).expect("a readable directory").flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(mentions(&path, needle));
        } else if String::from_utf8_lossy(&fs::read(&path).expect("a readable file"))
            .contains(needle)
        {
            found.push(path);
        }
    }
    found
}

#[test]
fn a_new_run_holds_only_the_workspace_and_its_own_configuration() {
    let scratch = scratch();
    let created = create(&scratch);
    let mut names: Vec<String> = fs::read_dir(&created.run.dir)
        .expect("the run directory")
        .map(|e| {
            e.expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert_eq!(names, ["config", "ws"]);
    assert_eq!(created.id.as_str().len(), 12);
    assert!(
        !created.run.dir.display().to_string().contains("toy"),
        "the path carries no label"
    );
    assert!(created.run.workspace().join("TASK.md").is_file());
    assert!(created.run.config().join("settings.json").is_file());
    assert!(created.operator.record().is_file());
    assert!(created.operator.manifest().is_file());
    let log = Command::new("git")
        .args(["log", "--format=%an %s"])
        .current_dir(created.run.workspace())
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .expect("git log runs");
    assert_eq!(
        String::from_utf8_lossy(&log.stdout).trim(),
        "runner Task pack"
    );
    assert_eq!(
        mentions(&created.run.dir, &scratch.benchy_root.display().to_string()),
        Vec::<PathBuf>::new()
    );
}

#[test]
fn a_run_root_inside_the_checkout_is_refused() {
    let scratch = scratch();
    let error = NewRun {
        manifest_path: scratch.manifest.clone(),
        benchy_root: scratch.benchy_root.clone(),
        run_root: scratch.benchy_root.join("runs"),
        task_pack: Some(repo().join("task")),
    }
    .create()
    .expect_err("the run root is inside the checkout");
    assert!(matches!(error, RunError::RootNotApart { .. }), "{error}");
}

fn finish(
    scratch: &Scratch,
    created: &CreatedRun,
    manual: Option<PathBuf>,
) -> Result<super::finish::Finished, RunError> {
    Finish {
        benchy_root: scratch.benchy_root.clone(),
        id: created.id.clone(),
        manual,
    }
    .run()
}

#[test]
fn finish_refuses_while_a_process_of_the_run_is_alive() {
    let scratch = scratch();
    let created = create(&scratch);
    let mut sleeper = Command::new("sleep")
        .arg("30")
        .current_dir(created.run.workspace())
        .spawn()
        .expect("sleep starts");
    let result = finish(&scratch, &created, None);
    sleeper.kill().expect("sleep stops");
    sleeper.wait().expect("sleep is reaped");
    match result {
        Err(RunError::StillAlive { pids }) => assert!(pids.contains(&sleeper.id()), "{pids:?}"),
        other => panic!("expected StillAlive, got {other:?}"),
    }
}

#[test]
fn finish_snapshots_the_head_and_falls_back_to_a_manual_entry() {
    let scratch = scratch();
    let created = create(&scratch);
    fs::write(created.run.workspace().join("stray.txt"), "not committed").expect("a stray file");
    let error = finish(&scratch, &created, None).expect_err("there are no transcripts");
    assert!(
        error.to_string().contains("write the numbers by hand"),
        "{error}"
    );
    let manual = scratch.benchy_root.join("manual.toml");
    fs::write(&manual, "cost_usd = 4.0\nbusy_seconds = 120.0\n").expect("the manual entry");
    let finished = finish(&scratch, &created, Some(manual)).expect("the manual entry is enough");
    assert_eq!(finished.telemetry.source, TelemetrySource::Manual);
    let done = finished.record.finish.expect("the finish is recorded");
    assert_eq!(done.head.map(|h| h.len()), Some(40));
    assert_eq!(done.uncommitted, ["?? stray.txt"]);
    assert!(created.operator.workspace_tarball().is_file());
    assert!(created.operator.telemetry().is_file());
}
