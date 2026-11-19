//! Seeded generators of engine input: fuzz streams, generated golden sessions and timing workloads.
//! Their shapes and seeds live in `validation/workloads/` and `validation/scenarios/generated.toml`.

mod rng;
mod stream;
mod workload;

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::conform;
use rng::Rng;
pub use stream::{Feature, stream};
use workload::{Scaling, Throughput};

#[derive(Debug, clap::Subcommand)]
pub enum GenCommand {
    /// Write the short fuzz streams, one folder for each feature set.
    Fuzz(OutputArgs),
    /// Write the generated golden sessions, one folder for each feature group.
    Golden(OutputArgs),
    /// Write one throughput workload to a file.
    Throughput {
        /// The workload's name in the configuration.
        name: String,
        /// The file to write.
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value = "validation/workloads/throughput.toml")]
        config: PathBuf,
    },
    /// Write the scaling probe: for each book size, `probe-SIZE.in` and its base `probe-SIZE-base.in`.
    Scaling {
        /// The folder to write.
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value = "validation/workloads/scaling.toml")]
        config: PathBuf,
    },
}

#[derive(Debug, clap::Args)]
pub struct OutputArgs {
    /// The folder to write.
    #[arg(long)]
    pub out: PathBuf,
    /// The configuration of shapes and seeds; each subcommand has its own default.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// A reference engine whose output is written beside each input as its expected `.out`.
    #[arg(long)]
    pub reference: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum GenError {
    #[error("failed to read {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {path}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("failed to write {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the reference engine failed on {path}: {reason}")]
    Reference { path: PathBuf, reason: String },
    #[error("{path} has no throughput workload named {name}")]
    UnknownWorkload { path: PathBuf, name: String },
}

/// Short independent streams, each restricted to one feature set.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FuzzConfig {
    pub seed: u64,
    pub streams: u64,
    pub min_lines: u64,
    pub max_lines: u64,
    #[serde(rename = "set")]
    pub sets: Vec<FeatureSet>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureSet {
    pub name: String,
    pub features: Vec<Feature>,
}

/// Generated golden sessions: each group's session `k` adds one more feature, in turn,
/// so that every group meets every other feature.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoldenConfig {
    pub seed: u64,
    pub sessions_per_group: u64,
    pub lines: u64,
    #[serde(rename = "group")]
    pub groups: Vec<FeatureSet>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThroughputConfig {
    #[serde(rename = "workload")]
    workloads: Vec<Throughput>,
}

/// A session to write: its path without extension, and its input.
pub struct GeneratedSession {
    pub path: PathBuf,
    pub input: String,
}

impl FuzzConfig {
    /// The streams in a fixed order; stream `i` goes to set `i` modulo the number of sets.
    pub fn sessions(&self) -> Vec<GeneratedSession> {
        (0..self.streams)
            .map(|index| {
                let set = &self.sets[usize::try_from(index).expect("small") % self.sets.len()];
                let mut rng = Rng::derive(self.seed, index);
                let lines = rng.between(self.min_lines, self.max_lines);
                GeneratedSession {
                    path: Path::new(&set.name).join(format!("fuzz-{index:04}")),
                    input: stream(&set.features, rng.next_u64(), to_usize(lines)),
                }
            })
            .collect()
    }
}

impl GoldenConfig {
    pub fn sessions(&self) -> Vec<GeneratedSession> {
        let mut sessions = Vec::new();
        for (group_index, group) in self.groups.iter().enumerate() {
            for index in 0..self.sessions_per_group {
                let extra = Feature::ALL[to_usize(index) % Feature::ALL.len()];
                let mut features = group.features.clone();
                if !features.contains(&extra) {
                    features.push(extra);
                }
                let stream_index = u64::try_from(group_index).expect("small") * 1000 + index;
                let seed = Rng::derive(self.seed, stream_index).next_u64();
                sessions.push(GeneratedSession {
                    path: Path::new(&group.name).join(format!("gen-{index:03}")),
                    input: stream(&features, seed, to_usize(self.lines)),
                });
            }
        }
        sessions
    }
}

fn to_usize(value: u64) -> usize {
    usize::try_from(value).expect("generator sizes fit in usize")
}

pub fn run(command: &GenCommand) -> Result<(), GenError> {
    match command {
        GenCommand::Fuzz(args) => {
            let config: FuzzConfig = load(config_or(args, "validation/workloads/fuzz.toml"))?;
            write_sessions(&config.sessions(), args)
        }
        GenCommand::Golden(args) => {
            let config: GoldenConfig =
                load(config_or(args, "validation/scenarios/generated.toml"))?;
            write_sessions(&config.sessions(), args)
        }
        GenCommand::Throughput { name, out, config } => {
            let workloads: ThroughputConfig = load(config)?;
            let workload = workloads
                .workloads
                .iter()
                .find(|workload| &workload.name == name)
                .ok_or_else(|| GenError::UnknownWorkload {
                    path: config.clone(),
                    name: name.clone(),
                })?;
            write_file(out, |file| workload.write(file))
        }
        GenCommand::Scaling { out, config } => {
            let probe: Scaling = load(config)?;
            create_dir(out)?;
            for &size in &probe.book_sizes {
                write_file(&out.join(format!("probe-{size}.in")), |file| {
                    probe.write(size, probe.operations, file)
                })?;
                write_file(&out.join(format!("probe-{size}-base.in")), |file| {
                    probe.write(size, 0, file)
                })?;
            }
            Ok(())
        }
    }
}

fn load<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, GenError> {
    let text = fs::read_to_string(path).map_err(|source| GenError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| GenError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

fn config_or<'a>(args: &'a OutputArgs, default: &'a str) -> &'a Path {
    args.config.as_deref().unwrap_or(Path::new(default))
}

fn create_dir(path: &Path) -> Result<(), GenError> {
    fs::create_dir_all(path).map_err(|source| GenError::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn write_file(
    path: &Path,
    write: impl FnOnce(&mut BufWriter<File>) -> std::io::Result<()>,
) -> Result<(), GenError> {
    let write_error = |source| GenError::Write {
        path: path.to_path_buf(),
        source,
    };
    let mut file = BufWriter::new(File::create(path).map_err(write_error)?);
    write(&mut file).map_err(write_error)?;
    file.flush().map_err(write_error)
}

fn write_sessions(sessions: &[GeneratedSession], args: &OutputArgs) -> Result<(), GenError> {
    for session in sessions {
        let base = args.out.join(&session.path);
        if let Some(parent) = base.parent() {
            create_dir(parent)?;
        }
        let input_path = base.with_extension("in");
        write_file(&input_path, |file| file.write_all(session.input.as_bytes()))?;
        if let Some(reference) = &args.reference {
            let output =
                conform::run_engine(reference, session.input.as_bytes(), Duration::from_secs(60))
                    .map_err(|reason| GenError::Reference {
                    path: input_path.clone(),
                    reason,
                })?;
            write_file(&base.with_extension("out"), |file| file.write_all(&output))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_config<T: serde::de::DeserializeOwned>(path: &str) -> T {
        load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap()
    }

    #[test]
    fn one_seed_gives_identical_fuzz_streams_twice() {
        let config: FuzzConfig = repo_config("validation/workloads/fuzz.toml");
        let first = config.sessions();
        let second = config.sessions();
        assert_eq!(first.len(), 2000);
        for (a, b) in first.iter().zip(&second) {
            assert_eq!((&a.path, &a.input), (&b.path, &b.input));
        }
    }

    #[test]
    fn the_committed_generated_sessions_match_their_seed() {
        let config: GoldenConfig = repo_config("validation/scenarios/generated.toml");
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("validation/scenarios");
        for session in config.sessions() {
            let path = root.join(&session.path).with_extension("in");
            let committed = fs::read_to_string(&path).unwrap();
            assert_eq!(committed, session.input, "{} differs", path.display());
        }
    }

    #[test]
    fn every_workload_configuration_parses() {
        let throughput: ThroughputConfig = repo_config("validation/workloads/throughput.toml");
        assert_eq!(throughput.workloads.len(), 3);
        let scaling: Scaling = repo_config("validation/workloads/scaling.toml");
        assert!(scaling.book_sizes.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
