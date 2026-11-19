//! Runs scoring inside the scorer image: builds the image, copies the workspace in, runs `benchy score --host` there and copies the results out.
//! The workspace is copied, not mounted, so file sharing between host and container adds no noise to the timings.

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use super::{ScoreArgs, ScoreError};
use benchy::contract::MeasuredScores;

/// The tag `benchy score` builds the scorer image under.
pub const IMAGE: &str = "benchy-scorer:local";

fn docker(arguments: &[&str]) -> Result<String, ScoreError> {
    let output = Command::new("docker")
        .args(arguments)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(ScoreError::io(format!(
            "start docker {}",
            arguments.join(" ")
        )))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(ScoreError::Command(format!(
            "docker {} exited with {}",
            arguments.join(" "),
            output.status
        )))
    }
}

/// Scores `workspace` in the scorer image, writes `metrics.json` and `score.log` beside `out`, and returns the scores.
///
/// # Errors
///
/// Fails when the image cannot be built or the container fails.
pub fn score(
    benchy: &Path,
    workspace: &Path,
    out: &Path,
    args: &ScoreArgs,
) -> Result<MeasuredScores, ScoreError> {
    let dockerfile = benchy.join("validation/Dockerfile").display().to_string();
    let context = benchy.display().to_string();
    eprintln!("building the scorer image {IMAGE}");
    // The container is made from the id this build printed, not from the tag, because another
    // checkout may build the same tag in the meantime.
    let image = docker(&["build", "--quiet", "-f", &dockerfile, "-t", IMAGE, &context])?;
    let mut inner = vec![
        "score",
        "--host",
        "--workspace",
        "/work/in",
        "--out",
        "/work/out/metrics.json",
    ];
    let only: Vec<String> = args.only.iter().map(|c| c.as_str().to_string()).collect();
    let only = only.join(",");
    if !only.is_empty() {
        inner.extend(["--only", &only]);
    }
    if args.par_out.is_some() {
        inner.extend(["--par-out", "/work/out/par.toml"]);
    }
    let name = format!("benchy-score-{}", std::process::id());
    let mut create = vec!["create", "--name", &name, &image];
    create.extend(inner);
    docker(&create)?;
    let result = run_container(&name, workspace, out, args);
    drop(docker(&["rm", "--force", &name]));
    result?;
    let text =
        fs::read_to_string(out).map_err(ScoreError::io(format!("read {}", out.display())))?;
    let mut scores: MeasuredScores =
        serde_json::from_str(&text).map_err(|error| ScoreError::Parse {
            path: out.to_path_buf(),
            reason: error.to_string(),
        })?;
    scores.scorer_image = Some(image);
    let json = serde_json::to_string_pretty(&scores).expect("measured scores always serialise");
    fs::write(out, json + "\n").map_err(ScoreError::io(format!("write {}", out.display())))?;
    Ok(scores)
}

fn run_container(
    name: &str,
    workspace: &Path,
    out: &Path,
    args: &ScoreArgs,
) -> Result<(), ScoreError> {
    let source = format!("{}/.", workspace.display());
    docker(&["cp", &source, &format!("{name}:/work/in")])?;
    let status = Command::new("docker")
        .args(["start", "--attach", name])
        .stdout(Stdio::null())
        .status()
        .map_err(ScoreError::io("start docker start".to_string()))?;
    let out_dir = out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(out_dir).map_err(ScoreError::io(format!("create {}", out_dir.display())))?;
    let copy = |file: &str, to: &Path| {
        docker(&[
            "cp",
            &format!("{name}:/work/out/{file}"),
            &to.display().to_string(),
        ])
    };
    drop(copy("score.log", &out.with_file_name("score.log")));
    if !status.success() {
        return Err(ScoreError::Command(format!(
            "scoring in the container exited with {status}"
        )));
    }
    copy("metrics.json", out)?;
    if let Some(par) = &args.par_out {
        copy("par.toml", par)?;
    }
    Ok(())
}
