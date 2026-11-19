//! Writes the rubric's numbers as the Markdown tables of `validation/VALIDATION.md`.
//! People read the document, so its tables must never drift from the numbers that score.

use super::points::CategoryId;
use super::rubric::{Rubric, Scale, Source};

const END_MARKER: &str = "<!-- /rubric -->";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TablesError {
    #[error("the document has no block <!-- rubric:{name} -->")]
    Missing { name: String },
    #[error("block <!-- rubric:{name} --> has no end marker {END_MARKER}")]
    Unterminated { name: String },
    #[error("the document has block <!-- rubric:{name} -->, which the rubric does not generate")]
    Unknown { name: String },
}

/// One generated table and the name of the block it fills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RubricTable {
    pub name: String,
    pub markdown: String,
}

impl Rubric {
    /// Every table of the scoring document, in document order.
    pub fn tables(&self) -> Vec<RubricTable> {
        let mut tables = vec![
            RubricTable {
                name: "formulas".to_string(),
                markdown: self.formulas_table(),
            },
            RubricTable {
                name: "measurement".to_string(),
                markdown: self.measurement_table(),
            },
            RubricTable {
                name: "categories".to_string(),
                markdown: self.categories_table(),
            },
        ];
        for id in CategoryId::ALL {
            tables.push(RubricTable {
                name: id.as_str().to_string(),
                markdown: self.metrics_table(id),
            });
        }
        tables
    }

    /// Returns `document` with each generated block replaced by the table the rubric gives now.
    ///
    /// # Errors
    ///
    /// Fails when a table has no block, a block has no end marker, or a block names no table.
    pub fn sync_tables(&self, document: &str) -> Result<String, TablesError> {
        let tables = self.tables();
        let mut synced = String::with_capacity(document.len());
        let mut rest = document;
        let mut seen = Vec::new();
        while let Some(start) = rest.find("<!-- rubric:") {
            let marker_end = rest[start..]
                .find("-->")
                .map(|i| start + i + 3)
                .unwrap_or(rest.len());
            let name = rest[start + "<!-- rubric:".len()..marker_end - 3]
                .trim()
                .to_string();
            let Some(table) = tables.iter().find(|t| t.name == name) else {
                return Err(TablesError::Unknown { name });
            };
            let Some(end) = rest[marker_end..].find(END_MARKER) else {
                return Err(TablesError::Unterminated { name });
            };
            synced.push_str(&rest[..marker_end]);
            synced.push('\n');
            synced.push_str(&table.markdown);
            rest = &rest[marker_end + end..];
            seen.push(name);
        }
        synced.push_str(rest);
        if let Some(missing) = tables.iter().find(|t| !seen.contains(&t.name)) {
            return Err(TablesError::Missing {
                name: missing.name.clone(),
            });
        }
        Ok(synced)
    }

    fn formulas_table(&self) -> String {
        let not_set = "set by the pilot".to_string();
        let efficiency = &self.efficiency;
        let rows = [
            (
                "quality.mean_weight",
                number(self.quality.mean_weight),
                "Weight of the mean of the three language scores in Quality",
            ),
            (
                "quality.lowest_weight",
                number(self.quality.lowest_weight),
                "Weight of the lowest language score in Quality",
            ),
            (
                "overall.base",
                number(self.overall.base),
                "Share of Quality that Overall keeps at Efficiency 0",
            ),
            (
                "overall.efficiency_weight",
                number(self.overall.efficiency_weight),
                "Share of Quality that Efficiency 100 adds",
            ),
            (
                "efficiency.cost_weight",
                number(efficiency.cost_weight),
                "Weight of cost in Efficiency",
            ),
            (
                "efficiency.time_weight",
                number(efficiency.time_weight),
                "Weight of elapsed busy time in Efficiency",
            ),
            (
                "efficiency.best_factor",
                number(efficiency.best_factor),
                "Multiple of the baseline median that scores 100",
            ),
            (
                "efficiency.worst_factor",
                number(efficiency.worst_factor),
                "Multiple of the baseline median that scores 0",
            ),
            (
                "efficiency.baseline_cost_usd",
                efficiency.baseline_cost_usd.map_or(not_set.clone(), number),
                "Median cost of the pilot baseline runs, in US dollars",
            ),
            (
                "efficiency.baseline_busy_seconds",
                efficiency.baseline_busy_seconds.map_or(not_set, number),
                "Median elapsed busy time of the pilot baseline runs, in seconds",
            ),
            (
                "correctness_scaling.floor",
                number(self.correctness_scaling.floor),
                "Golden pass rate at or below which scaled categories earn nothing",
            ),
            (
                "correctness_scaling.span",
                number(self.correctness_scaling.span),
                "Rise in pass rate above the floor that gives full credit",
            ),
            (
                "correctness_scaling.categories",
                letters(&self.correctness_scaling.categories),
                "Categories multiplied by the correctness factor",
            ),
            (
                "build_gate.categories",
                letters(&self.build_gate.categories),
                "Categories that score 0 when a language does not build",
            ),
            (
                "comment_density.density",
                number(self.comment_density.density),
                "Comment density above which the cap applies",
            ),
            (
                "comment_density.factor",
                number(self.comment_density.factor),
                "Share of its points that the capped metric can still earn",
            ),
            (
                "comment_density.metric",
                format!("`{}`", self.comment_density.metric),
                "Metric that the cap applies to",
            ),
            (
                "caps.cost_usd",
                number(self.caps.cost_usd),
                "Cost in US dollars at which a headless run stops",
            ),
            (
                "caps.wall_hours",
                number(self.caps.wall_hours),
                "Wall-clock hours after which a headless run stops",
            ),
        ];
        let mut table = String::from("| Key | Value | Meaning |\n|---|---|---|\n");
        for (key, value, meaning) in rows {
            table.push_str(&format!("| `{key}` | {value} | {meaning} |\n"));
        }
        table
    }

    fn measurement_table(&self) -> String {
        let judge = &self.judge;
        let measure = &self.measurement;
        let rows = [
            (
                "judge.model",
                judge.model.clone(),
                "Model that every judgement uses",
            ),
            (
                "judge.samples",
                judge.samples.to_string(),
                "Independent judgements per item; the median counts",
            ),
            (
                "judge.scale_max",
                judge.scale_max.to_string(),
                "Highest level of the judge scale, which starts at 0",
            ),
            (
                "measurement.fuzz_streams",
                measure.fuzz_streams.to_string(),
                "Seeded short fuzz streams per language",
            ),
            (
                "measurement.mutants_per_language",
                measure.mutants_per_language.to_string(),
                "Seeded mutants sampled from each core library",
            ),
            (
                "measurement.test_runs",
                measure.test_runs.to_string(),
                "Runs of each test suite for the stability check",
            ),
            (
                "measurement.performance_runs",
                measure.performance_runs.to_string(),
                "Alternating runs of reference and candidate per workload",
            ),
            (
                "measurement.min_workload_seconds",
                number(measure.min_workload_seconds),
                "Shortest run time of a workload on the reference",
            ),
            (
                "measurement.max_cyclomatic",
                measure.max_cyclomatic.to_string(),
                "Cyclomatic complexity above which a function counts as complex",
            ),
            (
                "measurement.max_function_lines",
                measure.max_function_lines.to_string(),
                "Lines above which a function counts as complex",
            ),
            (
                "measurement.max_nesting",
                measure.max_nesting.to_string(),
                "Nesting depth above which a function counts as complex",
            ),
            (
                "measurement.memory_limit_mb",
                measure.memory_limit_mb.to_string(),
                "Peak memory bound for any hostile input, in MB",
            ),
        ];
        let mut table = String::from("| Key | Value | Meaning |\n|---|---|---|\n");
        for (key, value, meaning) in rows {
            table.push_str(&format!("| `{key}` | {value} | {meaning} |\n"));
        }
        table
    }

    fn categories_table(&self) -> String {
        let mut table =
            String::from("| | Category | Points | Measured | Judged |\n|---|---|---|---|---|\n");
        for category in &self.categories {
            let from = |source| {
                category
                    .metrics
                    .iter()
                    .filter(|m| m.source == source)
                    .map(|m| m.points)
                    .sum::<f64>()
            };
            table.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                category.id.as_str(),
                category.name,
                number(category.points),
                number(from(Source::Measured)),
                number(from(Source::Judged)),
            ));
        }
        let total: f64 = self.categories.iter().map(|c| c.points).sum();
        table.push_str(&format!(
            "| | Total | {} | {} | {} |\n",
            number(total),
            number(self.points_from(Source::Measured)),
            number(self.points_from(Source::Judged)),
        ));
        table
    }

    fn metrics_table(&self, id: CategoryId) -> String {
        let mut table = String::from(
            "| Metric | Points | Source | Measure | Full at | Zero at | Scale |\n|---|---|---|---|---|---|---|\n",
        );
        for metric in &self.category(id).metrics {
            let source = match metric.source {
                Source::Measured => "measured",
                Source::Judged => "judged",
            };
            let scale = match metric.scale {
                Scale::Linear => "linear",
                Scale::Log => "log",
            };
            table.push_str(&format!(
                "| `{}.{}` {} | {} | {} | {} | {} | {} | {} |\n",
                id.as_str(),
                metric.id,
                metric.name,
                number(metric.points),
                source,
                metric.measure,
                number(metric.full),
                number(metric.zero),
                scale,
            ));
        }
        table
    }
}

fn number(value: f64) -> String {
    // Adding 0.0 turns the -0.0 that an empty sum gives into 0.0, so the table never shows "-0".
    (value + 0.0).to_string()
}

fn letters(ids: &[CategoryId]) -> String {
    ids.iter()
        .map(|id| id.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn validation_document_tables_match_the_rubric() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let rubric = Rubric::load(&root.join("validation/rubric.toml")).expect("valid rubric");
        let path = root.join("validation/VALIDATION.md");
        let document = std::fs::read_to_string(&path).expect("VALIDATION.md is readable");
        let synced = rubric
            .sync_tables(&document)
            .expect("every table has its block");
        if std::env::var_os("BENCHY_BLESS").is_some() {
            std::fs::write(&path, &synced).expect("VALIDATION.md is writable");
            return;
        }
        assert!(
            synced == document,
            "the tables of validation/VALIDATION.md differ from validation/rubric.toml; \
             run `BENCHY_BLESS=1 cargo test validation_document` to regenerate them"
        );
    }

    #[test]
    fn a_missing_block_is_reported() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let rubric = Rubric::load(&root.join("validation/rubric.toml")).expect("valid rubric");
        assert_eq!(
            rubric.sync_tables("no tables here\n"),
            Err(TablesError::Missing {
                name: "formulas".to_string()
            })
        );
    }

    #[test]
    fn an_edited_table_is_put_back() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let rubric = Rubric::load(&root.join("validation/rubric.toml")).expect("valid rubric");
        let mut document = String::new();
        for table in rubric.tables() {
            document.push_str(&format!(
                "<!-- rubric:{} -->\nstale\n{END_MARKER}\n",
                table.name
            ));
        }
        let synced = rubric
            .sync_tables(&document)
            .expect("every block is present");
        assert!(!synced.contains("stale"));
        assert_eq!(rubric.sync_tables(&synced), Ok(synced.clone()));
    }
}
