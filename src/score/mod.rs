//! `benchy score`: builds a workspace from clean, measures the measured metrics of each language and writes `metrics.json`.
//! It runs inside the scorer image by default; `--host` runs the same steps with the tools of this machine.

mod analysis;
mod container;
mod correctness;
mod delivery;
mod economy;
mod maintainability;
mod par;
mod performance;
mod process;
mod robustness;
pub(crate) mod sources;
mod suite;
pub(crate) mod test_code;
pub(crate) mod workspace;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::run::layout::{OperatorDir, RunDir, RunId, benchy_root, run_root};
use benchy::contract::{BuildOutcome, MetricScore};
use benchy::contract::{CategoryId, Language};
use benchy::contract::{MEASURED_FILE, MEASURED_SCHEMA, MeasuredLanguage, MeasuredScores};
use benchy::contract::{Rubric, RubricError};
use process::Log;

#[derive(Debug, clap::Args)]
pub struct ScoreArgs {
    /// Id of a finished run, as `benchy new` or `benchy run` printed it.
    #[arg(required_unless_present_any = ["workspace", "reference"])]
    pub run_id: Option<String>,
    /// Score this workspace folder instead of a run's workspace.
    #[arg(long, conflicts_with_all = ["run_id", "reference"])]
    pub workspace: Option<PathBuf>,
    /// Score the reference workspace: the task pack with the three reference solutions.
    #[arg(long, conflicts_with = "run_id")]
    pub reference: bool,
    /// Where to write metrics.json. Defaults to the run's operator data, or ./metrics.json.
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Run on this machine's toolchains instead of inside the scorer image.
    #[arg(long)]
    pub host: bool,
    /// Measure only these categories, such as A,B,E.
    #[arg(long, value_delimiter = ',', value_parser = parse_category)]
    pub only: Vec<CategoryId>,
    /// Also write the reference measurements that the rubric uses as par to this file. Needs --reference.
    #[arg(long, requires = "reference")]
    pub par_out: Option<PathBuf>,
    /// Folder for the scored copy and the generated inputs. Defaults to a fresh temporary folder.
    #[arg(long)]
    pub scratch: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum ScoreError {
    #[error("failed to {action}")]
    Io {
        action: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Command(String),
    #[error("failed to load the rubric")]
    Rubric(#[from] RubricError),
    #[error("failed to read {path}: {reason}")]
    Parse { path: PathBuf, reason: String },
}

fn parse_category(text: &str) -> Result<CategoryId, String> {
    CategoryId::ALL
        .into_iter()
        .find(|category| category.as_str().eq_ignore_ascii_case(text))
        .ok_or_else(|| format!("{text} is not a category from A to I"))
}

impl ScoreError {
    fn io(action: String) -> impl FnOnce(std::io::Error) -> ScoreError {
        move |source| ScoreError::Io { action, source }
    }
}

/// Runs `benchy score`.
///
/// # Errors
///
/// Fails when the workspace or rubric cannot be read, the scorer image cannot run, or the output cannot be written.
pub fn run(run_root_flag: Option<&Path>, args: &ScoreArgs) -> Result<(), ScoreError> {
    let benchy = benchy_root().map_err(|error| ScoreError::Command(error.to_string()))?;
    let scratch = Scratch::new(args.scratch.as_deref())?;
    let (workspace, out) = target(run_root_flag, &benchy, args, scratch.path())?;
    if args.host {
        let scores = score_on_host(&benchy, &workspace, scratch.path(), args)?;
        write_scores(&scores, &out)
    } else {
        let scores = container::score(&benchy, &workspace, &out, args)?;
        write_scores(&scores, &out)
    }
}

/// Scores `workspace` in `only` these categories, in the scorer image unless `host`, and writes its `metrics.json` to `out`.
///
/// # Errors
///
/// Fails as [`run`] does.
pub fn score_workspace(
    benchy: &Path,
    workspace: &Path,
    out: &Path,
    only: &[CategoryId],
    host: bool,
) -> Result<MeasuredScores, ScoreError> {
    let args = ScoreArgs {
        run_id: None,
        workspace: Some(workspace.to_path_buf()),
        reference: false,
        out: Some(out.to_path_buf()),
        host,
        only: only.to_vec(),
        par_out: None,
        scratch: None,
    };
    if host {
        let scratch = Scratch::new(None)?;
        let scores = score_on_host(benchy, workspace, scratch.path(), &args)?;
        let json = serde_json::to_string_pretty(&scores).expect("measured scores always serialise");
        fs::write(out, json + "\n").map_err(ScoreError::io(format!("write {}", out.display())))?;
        Ok(scores)
    } else {
        container::score(benchy, workspace, out, &args)
    }
}

/// The workspace to score and where its `metrics.json` goes.
fn target(
    run_root_flag: Option<&Path>,
    benchy: &Path,
    args: &ScoreArgs,
    scratch: &Path,
) -> Result<(PathBuf, PathBuf), ScoreError> {
    let default_out = || {
        args.out
            .clone()
            .unwrap_or_else(|| PathBuf::from(MEASURED_FILE))
    };
    if let Some(dir) = &args.workspace {
        return Ok((dir.clone(), default_out()));
    }
    if args.reference {
        let dir = scratch.join("reference-workspace");
        workspace::assemble_reference(benchy, &dir)?;
        return Ok((dir, default_out()));
    }
    let id_text = args.run_id.as_deref().unwrap_or_default();
    let id = RunId::parse(id_text).map_err(|error| ScoreError::Command(error.to_string()))?;
    let root = run_root(run_root_flag).map_err(|error| ScoreError::Command(error.to_string()))?;
    let operator = OperatorDir::new(benchy, &id);
    let out = args
        .out
        .clone()
        .unwrap_or_else(|| operator.dir.join(MEASURED_FILE));
    Ok((RunDir::new(&root, &id).workspace(), out))
}

fn write_scores(scores: &MeasuredScores, out: &Path) -> Result<(), ScoreError> {
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(ScoreError::io(format!("create {}", parent.display())))?;
    }
    let json = serde_json::to_string_pretty(scores).expect("measured scores always serialise");
    fs::write(out, json + "\n").map_err(ScoreError::io(format!("write {}", out.display())))?;
    eprintln!("wrote {}", out.display());
    print_table(scores);
    Ok(())
}

fn print_table(scores: &MeasuredScores) {
    println!(
        "{:<8} {:<22} {:>14} {:>8} {:>6}",
        "language", "metric", "raw", "points", "max"
    );
    for language in &scores.languages {
        for metric in &language.metrics {
            let raw = metric
                .raw
                .map_or_else(|| "-".to_string(), |raw| format!("{raw:.4}"));
            println!(
                "{:<8} {:<22} {:>14} {:>8.3} {:>6}",
                language.language.as_str(),
                metric.key,
                raw,
                metric.points,
                metric.max
            );
        }
    }
}

/// A scratch folder that is removed at the end unless the operator named it.
struct Scratch {
    path: PathBuf,
    owned: bool,
}

impl Scratch {
    fn new(named: Option<&Path>) -> Result<Scratch, ScoreError> {
        let (path, owned) = match named {
            Some(path) => (path.to_path_buf(), false),
            None => (
                std::env::temp_dir().join(format!("benchy-score-{}", std::process::id())),
                true,
            ),
        };
        fs::create_dir_all(&path).map_err(ScoreError::io(format!("create {}", path.display())))?;
        Ok(Scratch { path, owned })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if self.owned {
            drop(fs::remove_dir_all(&self.path));
        }
    }
}

/// What every measurement of one language needs.
pub struct Context<'a> {
    pub rubric: &'a Rubric,
    pub language: Language,
    /// The prepared tree, built with the pristine `Makefile`.
    pub root: &'a Path,
    pub benchy: &'a Path,
    pub scratch: &'a Path,
    pub engine: PathBuf,
    pub reference_engine: PathBuf,
    pub rust_reference_engine: PathBuf,
    pub par: &'a par::Par,
}

/// The measured metrics of one language, as they are collected.
#[derive(Debug, Default)]
pub struct Metrics {
    pub list: Vec<MetricScore>,
    pub notes: Vec<String>,
}

impl Metrics {
    /// Records `raw` for the metric `key` and the points the rubric gives it. A missing value earns 0.
    pub fn add(&mut self, rubric: &Rubric, key: &str, raw: Option<f64>) {
        let metric = rubric
            .metric(key)
            .unwrap_or_else(|| panic!("the rubric defines {key}"));
        let points = raw.map_or(0.0, |raw| metric.points_for(raw));
        self.list.push(MetricScore {
            key: key.to_string(),
            raw,
            points,
            max: metric.points,
        });
    }

    /// Records a metric whose points are not a function of one raw value, such as a mean over workloads.
    pub fn add_points(&mut self, rubric: &Rubric, key: &str, raw: Option<f64>, points: f64) {
        let metric = rubric
            .metric(key)
            .unwrap_or_else(|| panic!("the rubric defines {key}"));
        self.list.push(MetricScore {
            key: key.to_string(),
            raw,
            points: points.clamp(0.0, metric.points),
            max: metric.points,
        });
    }

    pub fn note(&mut self, text: String) {
        self.notes.push(text);
    }
}

/// The program that `make build-<language>` writes.
fn engine_path(root: &Path, language: Language) -> PathBuf {
    let name = if language == Language::CSharp {
        "Engine"
    } else {
        "engine"
    };
    root.join("bin").join(language.as_str()).join(name)
}

fn wanted(args: &ScoreArgs, category: CategoryId) -> bool {
    args.only.is_empty() || args.only.contains(&category)
}

/// Scores `workspace` with the tools on this machine.
///
/// # Errors
///
/// Fails when the rubric cannot be read or the workspace cannot be prepared.
pub fn score_on_host(
    benchy: &Path,
    workspace: &Path,
    scratch: &Path,
    args: &ScoreArgs,
) -> Result<MeasuredScores, ScoreError> {
    let mut log = Log::default();
    let rubric = Rubric::load(&benchy.join("validation/rubric.toml"))?;
    let par = par::Par::load_or_measure(benchy)?;
    let prepared = workspace::prepare(workspace, &benchy.join("task"), &scratch.join("scored"))?;
    log.line(&format!(
        "scoring {} ({})",
        workspace.display(),
        prepared.hash
    ));
    let references = scratch.join("references");
    workspace::assemble_reference(benchy, &references)?;
    for language in Language::ALL {
        process::shell(
            &references,
            &format!("make build-{}", language.as_str()),
            &mut log,
        );
    }
    process::shell(&prepared.root, "make clean", &mut log);
    let mut languages = Vec::new();
    let mut par_rows = Vec::new();
    let git_state = delivery::git_state(workspace);
    let delivery_results = if wanted(args, CategoryId::H) {
        // The commands run in their own copy, so whatever they build or clean cannot touch the scored tree.
        let copy = scratch.join("delivery");
        crate::run::workspace::copy_tree(&prepared.root, &copy)
            .map_err(|error| ScoreError::Command(error.to_string()))?;
        delivery::run_delivery_commands(&copy, &mut log)
    } else {
        Vec::new()
    };
    for language in Language::ALL {
        let built = process::shell(
            &prepared.root,
            &format!("make build-{}", language.as_str()),
            &mut log,
        );
        let engine = engine_path(&prepared.root, language);
        let build = if built.success && engine.is_file() {
            BuildOutcome::Built
        } else {
            BuildOutcome::Failed
        };
        let context = Context {
            rubric: &rubric,
            language,
            root: &prepared.root,
            benchy,
            scratch,
            engine,
            reference_engine: engine_path(&references, language),
            rust_reference_engine: engine_path(&references, Language::Rust),
            par: &par,
        };
        let mut metrics = Metrics::default();
        let mut golden = correctness::Golden::default();
        if build == BuildOutcome::Built {
            if wanted(args, CategoryId::A) {
                golden = correctness::measure(&context, &mut metrics, &mut log);
            }
            if wanted(args, CategoryId::B) {
                robustness::measure(&context, &mut metrics, &mut log);
            }
            if wanted(args, CategoryId::C) {
                let reference = performance::measure(&context, &mut metrics, &mut log);
                par_rows.push((language, reference));
            }
        } else {
            metrics.note(format!(
                "make build-{} failed, so A to D were not measured",
                language.as_str()
            ));
        }
        let files = sources::production_files(&prepared.root, language);
        let measures_d = build == BuildOutcome::Built && wanted(args, CategoryId::D);
        if measures_d || wanted(args, CategoryId::E) || wanted(args, CategoryId::F) {
            let findings = analysis::analyse(&prepared.root, language, &mut log);
            if measures_d {
                suite::measure(&context, &files, &findings, &mut metrics, &mut log);
            }
            if wanted(args, CategoryId::E) {
                economy::measure(&context, &files, &findings, &mut metrics, &mut log);
            }
            if wanted(args, CategoryId::F) {
                maintainability::measure(&context, &files, &findings, &mut metrics, &mut log);
            }
        }
        if wanted(args, CategoryId::H) {
            delivery::measure_contract(&context, &files, &delivery_results, &mut metrics, &mut log);
        }
        if wanted(args, CategoryId::I) {
            delivery::measure_hygiene(&context, &git_state, &mut metrics, &mut log);
        }
        languages.push(MeasuredLanguage {
            language,
            build,
            golden_pass_rate: golden.rate,
            metrics: metrics.list,
            groups: golden.groups,
            comment_density: sources::comment_density(&files),
            notes: metrics.notes,
        });
    }
    if let Some(path) = &args.par_out {
        par::write(path, benchy, &par_rows)?;
    }
    if let Some(out) = &args.out {
        let log_path = out.with_file_name("score.log");
        drop(fs::write(log_path, &log.text));
    }
    Ok(MeasuredScores {
        schema: MEASURED_SCHEMA,
        workspace: prepared.hash,
        scorer_image: None,
        languages,
    })
}

/// The time any one engine run may take before it counts as hung.
const RUN_TIMEOUT: Duration = Duration::from_secs(120);

/// Applies `work` to every item on all cores and returns the results in the order of `items`.
fn parallel_map<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(Vec::with_capacity(items.len()));
    let workers = std::thread::available_parallelism().map_or(4, usize::from);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(item) = items.get(index) else { break };
                    let result = work(item);
                    results
                        .lock()
                        .expect("no worker panics while it holds the lock")
                        .push((index, result));
                }
            });
        }
    });
    let mut results = results.into_inner().expect("every worker finished");
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}
