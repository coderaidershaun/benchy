//! Category C: throughput, scaling and peak memory, each against the reference of the same language in alternating runs.
//! Start-up on empty input is subtracted, the median of the runs counts, and a workload whose output differs earns nothing.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use super::process::{self, Finished, Input, Log, Output};
use super::{Context, Metrics};
use crate::r#gen::{self, GenCommand};

/// The reference measurements of one language, which `par.toml` records.
#[derive(Debug, Clone, Default)]
pub struct ReferenceTimes {
    pub startup_seconds: f64,
    pub throughput_seconds: Vec<(String, f64)>,
    pub peak_memory_mb: f64,
    pub scaling_exponent: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ThroughputNames {
    workload: Vec<Named>,
}

#[derive(Debug, Deserialize)]
struct Named {
    name: String,
}

#[derive(Debug, Deserialize)]
struct ScalingSizes {
    operations: u64,
    book_sizes: Vec<u64>,
}

/// Times of one workload: the reference and candidate run times, startup subtracted, and peak memory.
struct Paired {
    reference: Vec<f64>,
    candidate: Vec<f64>,
    valid: bool,
    reference_peak: u64,
    candidate_peak: u64,
}

pub fn measure(context: &Context, metrics: &mut Metrics, log: &mut Log) -> ReferenceTimes {
    let rubric = context.rubric;
    let runs = usize::try_from(rubric.measurement.performance_runs).unwrap_or(5);
    let dir = context.scratch.join("workloads");
    let workloads = match generate(context.benchy, &dir) {
        Ok(workloads) => workloads,
        Err(error) => {
            log.line(&format!("workloads could not be generated: {error}"));
            metrics.add(rubric, "C.throughput", None);
            metrics.add(rubric, "C.scaling", None);
            metrics.add(rubric, "C.peak_memory", None);
            return ReferenceTimes::default();
        }
    };
    let language = context.language.as_str();
    let reference_startup = startup(&context.reference_engine, runs);
    let candidate_startup = startup(&context.engine, runs);
    log.line(&format!(
        "{language} start-up: reference {reference_startup:.4}s, candidate {candidate_startup:.4}s"
    ));
    let mut times = ReferenceTimes {
        startup_seconds: reference_startup,
        ..ReferenceTimes::default()
    };
    let throughput_metric = rubric
        .metric("C.throughput")
        .expect("the rubric defines C.throughput");
    let mut points = Vec::new();
    let mut ratios = Vec::new();
    let mut reference_peak = 0;
    let mut candidate_peak = 0;
    for (name, input) in &workloads.throughput {
        let paired = paired_runs(context, input, runs, (reference_startup, candidate_startup));
        let reference = median(&paired.reference);
        let candidate = median(&paired.candidate);
        let ratio = candidate / reference.max(1e-9);
        log.line(&format!(
            "{language} throughput {name}: reference {reference:.3}s (spread {:.1}%), candidate {candidate:.3}s (spread {:.1}%), ratio {ratio:.3}, identical output {}",
            spread(&paired.reference) * 100.0,
            spread(&paired.candidate) * 100.0,
            paired.valid
        ));
        metrics.note(format!(
            "C.throughput {name}: ratio {ratio:.3}, spread reference {:.1}% candidate {:.1}%",
            spread(&paired.reference) * 100.0,
            spread(&paired.candidate) * 100.0
        ));
        times.throughput_seconds.push((name.clone(), reference));
        reference_peak = reference_peak.max(paired.reference_peak);
        if paired.valid {
            points.push(throughput_metric.points_for(ratio));
            ratios.push(ratio);
            candidate_peak = candidate_peak.max(paired.candidate_peak);
        } else {
            points.push(0.0);
            metrics.note(format!(
                "C.throughput {name}: output differs from the reference, so it earns nothing"
            ));
        }
    }
    let mean_points = points.iter().sum::<f64>() / points.len().max(1) as f64;
    let geometric = (!ratios.is_empty())
        .then(|| (ratios.iter().map(|r| r.ln()).sum::<f64>() / ratios.len() as f64).exp());
    metrics.add_points(rubric, "C.throughput", geometric, mean_points);
    let memory = (candidate_peak > 0 && reference_peak > 0)
        .then(|| candidate_peak as f64 / reference_peak as f64);
    times.peak_memory_mb = reference_peak as f64 / (1024.0 * 1024.0);
    log.line(&format!(
        "{language} peak memory: reference {} MB, candidate {} MB",
        reference_peak / (1024 * 1024),
        candidate_peak / (1024 * 1024)
    ));
    metrics.add(rubric, "C.peak_memory", memory);
    let (reference_exponent, candidate_exponent) = scaling(context, &workloads, runs, log);
    times.scaling_exponent = reference_exponent;
    metrics.add(rubric, "C.scaling", candidate_exponent);
    times
}

struct Workloads {
    throughput: Vec<(String, PathBuf)>,
    /// For each book size: the probe, its base and the number of operations between them.
    scaling: Vec<(u64, PathBuf, PathBuf)>,
    operations: u64,
}

fn generate(benchy: &Path, dir: &Path) -> Result<Workloads, String> {
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let throughput_config = benchy.join("validation/workloads/throughput.toml");
    let scaling_config = benchy.join("validation/workloads/scaling.toml");
    let read = |path: &Path| {
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
    };
    let names: ThroughputNames =
        toml::from_str(&read(&throughput_config)?).map_err(|e| e.to_string())?;
    let sizes: ScalingSizes = toml::from_str(&read(&scaling_config)?).map_err(|e| e.to_string())?;
    let mut throughput = Vec::new();
    for Named { name } in names.workload {
        let out = dir.join(format!("{name}.in"));
        if !out.is_file() {
            r#gen::run(&GenCommand::Throughput {
                name: name.clone(),
                out: out.clone(),
                config: throughput_config.clone(),
            })
            .map_err(|error| error.to_string())?;
        }
        throughput.push((name, out));
    }
    let scaling_dir = dir.join("scaling");
    if !scaling_dir.is_dir() {
        r#gen::run(&GenCommand::Scaling {
            out: scaling_dir.clone(),
            config: scaling_config,
        })
        .map_err(|error| error.to_string())?;
    }
    let scaling = sizes
        .book_sizes
        .iter()
        .map(|&size| {
            (
                size,
                scaling_dir.join(format!("probe-{size}.in")),
                scaling_dir.join(format!("probe-{size}-base.in")),
            )
        })
        .collect();
    Ok(Workloads {
        throughput,
        scaling,
        operations: sizes.operations,
    })
}

fn startup(engine: &Path, runs: usize) -> f64 {
    let times: Vec<f64> = (0..runs)
        .filter_map(|_| {
            process::run(
                engine,
                &[],
                &Input::Bytes(b""),
                &Output::Keep,
                Duration::from_secs(60),
            )
            .ok()
        })
        .map(|run| run.elapsed.as_secs_f64())
        .collect();
    median(&times)
}

/// A first reference run writes the expected output; then reference and candidate alternate `runs` times each.
fn paired_runs(context: &Context, input: &Path, runs: usize, startups: (f64, f64)) -> Paired {
    let expected = input.with_extension(format!("{}.expected", context.language.as_str()));
    let warm_up = timed(
        &context.reference_engine,
        input,
        &Output::WriteTo(&expected),
        Duration::from_secs(600),
    );
    let budget = warm_up.as_ref().map_or(Duration::from_secs(600), |run| {
        (run.elapsed * 20 + Duration::from_secs(60)).min(Duration::from_secs(900))
    });
    let mut paired = Paired {
        reference: Vec::new(),
        candidate: Vec::new(),
        valid: warm_up.is_some_and(|run| run.exited_cleanly()),
        reference_peak: 0,
        candidate_peak: 0,
    };
    for _ in 0..runs {
        let reference = timed(
            &context.reference_engine,
            input,
            &Output::CompareWith(&expected),
            budget,
        );
        let candidate = timed(
            &context.engine,
            input,
            &Output::CompareWith(&expected),
            budget,
        );
        if let Some(run) = reference {
            paired
                .reference
                .push((run.elapsed.as_secs_f64() - startups.0).max(0.0));
            paired.reference_peak = paired.reference_peak.max(run.peak_rss_bytes);
        }
        match candidate {
            Some(run) if run.exited_cleanly() && run.matches == Some(true) => {
                paired
                    .candidate
                    .push((run.elapsed.as_secs_f64() - startups.1).max(0.0));
                paired.candidate_peak = paired.candidate_peak.max(run.peak_rss_bytes);
            }
            _ => {
                paired.valid = false;
                // A wrong or failed run will not get better; the rest of the runs would only take time.
                break;
            }
        }
    }
    drop(fs::remove_file(&expected));
    paired
}

fn timed(engine: &Path, input: &Path, output: &Output, timeout: Duration) -> Option<Finished> {
    process::run(engine, &[], &Input::File(input), output, timeout).ok()
}

/// The growth exponent of time per operation with book size, for the reference and the candidate.
fn scaling(
    context: &Context,
    workloads: &Workloads,
    runs: usize,
    log: &mut Log,
) -> (Option<f64>, Option<f64>) {
    let mut reference_points = Vec::new();
    let mut candidate_points = Vec::new();
    let mut valid = true;
    let operations = workloads.operations as f64;
    for (size, probe, base) in &workloads.scaling {
        let probe_times = paired_runs(context, probe, runs, (0.0, 0.0));
        let base_times = paired_runs(context, base, runs, (0.0, 0.0));
        valid = valid && probe_times.valid && base_times.valid;
        if !valid {
            log.line(&format!(
                "{} scaling probe {size}: output differs from the reference",
                context.language.as_str()
            ));
            break;
        }
        let per_operation =
            |probe: &[f64], base: &[f64]| ((median(probe) - median(base)) / operations).max(1e-12);
        let reference = per_operation(&probe_times.reference, &base_times.reference);
        let candidate = per_operation(&probe_times.candidate, &base_times.candidate);
        log.line(&format!(
            "{} scaling probe {size}: reference {:.1} ns per operation, candidate {:.1} ns per operation",
            context.language.as_str(),
            reference * 1e9,
            candidate * 1e9
        ));
        reference_points.push((*size as f64, reference));
        candidate_points.push((*size as f64, candidate));
    }
    let reference = slope(&reference_points);
    let candidate = if valid {
        slope(&candidate_points)
    } else {
        None
    };
    log.line(&format!(
        "{} scaling exponent: reference {reference:?}, candidate {candidate:?}",
        context.language.as_str()
    ));
    (reference, candidate)
}

/// The least-squares slope of log(time per operation) against log(book size).
pub fn slope(points: &[(f64, f64)]) -> Option<f64> {
    if points.len() < 2 {
        return None;
    }
    let logs: Vec<(f64, f64)> = points.iter().map(|(x, y)| (x.ln(), y.ln())).collect();
    let count = logs.len() as f64;
    let mean_x = logs.iter().map(|(x, _)| x).sum::<f64>() / count;
    let mean_y = logs.iter().map(|(_, y)| y).sum::<f64>() / count;
    let covariance: f64 = logs.iter().map(|(x, y)| (x - mean_x) * (y - mean_y)).sum();
    let variance: f64 = logs.iter().map(|(x, _)| (x - mean_x).powi(2)).sum();
    (variance > 0.0).then(|| covariance / variance)
}

pub fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[middle]
    } else {
        f64::midpoint(sorted[middle - 1], sorted[middle])
    }
}

/// The range of the runs as a share of their median.
fn spread(values: &[f64]) -> f64 {
    let low = values.iter().copied().fold(f64::INFINITY, f64::min);
    let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if values.is_empty() {
        0.0
    } else {
        (high - low) / median(values).max(1e-9)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_linear_cost_has_exponent_one_and_a_constant_cost_zero() {
        let linear = [(1000.0, 1e-6), (10_000.0, 1e-5), (100_000.0, 1e-4)];
        let constant = [(1000.0, 3e-7), (10_000.0, 3e-7), (100_000.0, 3e-7)];

        assert!(slope(&linear).is_some_and(|s| (s - 1.0).abs() < 1e-9));
        assert!(slope(&constant).is_some_and(|s| s.abs() < 1e-9));
    }

    #[test]
    fn the_median_of_an_even_count_is_the_mean_of_the_middle_pair() {
        assert!((median(&[4.0, 1.0, 3.0, 2.0]) - 2.5).abs() < 1e-12);
        assert!((median(&[5.0, 1.0, 3.0]) - 3.0).abs() < 1e-12);
    }
}
