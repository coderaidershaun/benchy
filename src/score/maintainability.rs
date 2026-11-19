//! Category F: lint findings per thousand lines, formatting, the share of complex functions and hazards.
//! Complexity comes from lizard for all three languages; nesting is the depth of braces inside each function it finds,
//! because lizard's own nesting count treats each Go `case` as one more level.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use super::analysis::{self, Findings};
use super::correctness::ratio;
use super::process::{self, Log};
use super::sources::{self, SourceFile};
use super::{Context, Metrics};
use benchy::contract::Measurement;

pub fn measure(
    context: &Context,
    files: &[SourceFile],
    findings: &Findings,
    metrics: &mut Metrics,
    log: &mut Log,
) {
    let rubric = context.rubric;
    // Dead code is scored once, in economy, so it neither dilutes these rates nor the share of complex functions.
    let unused: Vec<BTreeSet<usize>> = files
        .iter()
        .map(|file| super::suite::unused_lines(file, findings))
        .collect();
    let dead_lines: usize = files
        .iter()
        .zip(&unused)
        .map(|(file, lines)| {
            lines
                .iter()
                .filter(|&&n| file.is_production_line(n))
                .count()
        })
        .sum();
    let live_lines = sources::production_line_total(files).saturating_sub(dead_lines);
    let thousands = live_lines as f64 / 1000.0;
    let per_thousand = |count: usize| (thousands > 0.0).then(|| count as f64 / thousands);
    metrics.add(rubric, "F.lint", per_thousand(findings.lint));
    metrics.add(
        rubric,
        "F.format",
        findings.unformatted.map(|count| count as f64),
    );
    let complex = complex_share(
        &context.scratch.join("lizard"),
        files,
        &unused,
        &rubric.measurement,
        log,
    );
    metrics.add(rubric, "F.complexity", complex);
    let hazards = analysis::hazards(files, context.language);
    log.line(&format!(
        "{} lint {}, unused {}, unformatted {:?}, hazards {hazards}",
        context.language.as_str(),
        findings.lint,
        findings.unused,
        findings.unformatted
    ));
    metrics.add(rubric, "F.hazards", per_thousand(hazards));
}

/// One function as lizard measures it.
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub code_lines: u32,
    pub cyclomatic: u32,
    pub nesting: u32,
}

/// The share of production functions over any limit; absent when lizard could not run.
/// Lizard reads copies in `dir` with comments, strings and quote marks blanked, because a byte
/// literal such as `b'0'` makes it run one Rust function on into the next.
fn complex_share(
    dir: &Path,
    files: &[SourceFile],
    unused: &[BTreeSet<usize>],
    limits: &Measurement,
    log: &mut Log,
) -> Option<f64> {
    if files.is_empty() {
        return Some(0.0);
    }
    for file in files {
        let copy = dir.join(&file.path);
        let blanked = file.code.replace(['\'', '"', '`'], " ");
        let written = copy
            .parent()
            .is_some_and(|parent| fs::create_dir_all(parent).is_ok())
            && fs::write(&copy, blanked).is_ok();
        if !written {
            log.line(&format!("could not write {}", copy.display()));
            return None;
        }
    }
    let root = dir;
    let list: Vec<String> = files
        .iter()
        .map(|f| format!("'{}'", f.path.display()))
        .collect();
    let output = process::shell(root, &format!("lizard --csv {}", list.join(" ")), log);
    if !output.success {
        return None;
    }
    let functions: Vec<Function> = output
        .stdout
        .lines()
        .filter_map(parse_function)
        .filter_map(|mut function| {
            let index = files
                .iter()
                .position(|file| file.path.display().to_string() == function.file)?;
            let file = &files[index];
            function.nesting = brace_nesting(&file.code, function.start, function.end);
            let live = !unused[index].contains(&function.start);
            (file.is_production_line(function.start) && live).then_some(function)
        })
        .collect();
    let complex: Vec<&Function> = functions
        .iter()
        .filter(|f| {
            f.cyclomatic > limits.max_cyclomatic
                || f.code_lines > limits.max_function_lines
                || f.nesting > limits.max_nesting
        })
        .collect();
    for function in &complex {
        log.line(&format!("complex function: {function:?}"));
    }
    log.line(&format!(
        "functions: {}, complex: {}",
        functions.len(),
        complex.len()
    ));
    Some(ratio(complex.len(), functions.len()))
}

/// Reads one line of `lizard --csv`: nloc, ccn, tokens, parameters, length, location, file, name,
/// long name, start and end. Quoted fields may hold commas.
pub fn parse_function(line: &str) -> Option<Function> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut field)),
            _ => field.push(c),
        }
    }
    fields.push(field);
    if fields.len() < 11 {
        return None;
    }
    Some(Function {
        file: fields[6].clone(),
        start: fields[9].parse().ok()?,
        end: fields[10].parse().ok()?,
        code_lines: fields[0].parse().ok()?,
        cyclomatic: fields[1].parse().ok()?,
        nesting: 0,
    })
}

/// How deep braces nest inside the body of the function on lines `start` to `end` of `code`,
/// with comments and strings already blanked. The body's own braces are level 0.
pub fn brace_nesting(code: &str, start: usize, end: usize) -> u32 {
    let mut depth: u32 = 0;
    let mut deepest: u32 = 0;
    for line in code
        .lines()
        .skip(start.saturating_sub(1))
        .take(end + 1 - start.min(end))
    {
        for c in line.chars() {
            match c {
                '{' => {
                    depth += 1;
                    deepest = deepest.max(depth);
                }
                '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    deepest.saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lizard_line_with_commas_in_quotes_is_read() {
        let line = "4,1,34,2,4,\"can_trade_at@45-48@rust/src/book.rs\",\"rust/src/book.rs\",\"can_trade_at\",\"can_trade_at & self , price : Price\",45,48";

        let function = parse_function(line);

        assert_eq!(
            function,
            Some(Function {
                file: "rust/src/book.rs".to_string(),
                start: 45,
                end: 48,
                code_lines: 4,
                cyclomatic: 1,
                nesting: 0,
            })
        );
    }

    #[test]
    fn nesting_counts_braces_inside_the_body() {
        let go = "func f(x int) int {\n\tswitch x {\n\tcase 1:\n\t\tif x > 0 {\n\t\t\treturn 1\n\t\t}\n\t}\n\treturn 0\n}\n";

        assert_eq!(brace_nesting(go, 1, 9), 2);
    }
}
