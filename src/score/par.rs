//! Par: the measurements of the three reference solutions that the rubric scores against, kept in `validation/par.toml`.
//! `benchy score --reference --par-out validation/par.toml` writes the file again from fresh measurements.

use std::fmt::Write;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::ScoreError;
use super::performance::ReferenceTimes;
use super::sources;
use benchy::contract::Language;

/// The par of one language that scoring reads. The file also records the reference timings, for the report.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LanguagePar {
    pub production_lines: usize,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Par {
    pub rust: LanguagePar,
    pub go: LanguagePar,
    pub csharp: LanguagePar,
}

impl Par {
    /// Reads `validation/par.toml`, or counts the production lines of the references when it is missing.
    ///
    /// # Errors
    ///
    /// Fails when the file exists but does not parse.
    pub fn load_or_measure(benchy: &Path) -> Result<Par, ScoreError> {
        let path = benchy.join("validation/par.toml");
        match fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).map_err(|error| ScoreError::Parse {
                path,
                reason: error.to_string(),
            }),
            Err(_) => {
                let mut par = Par::default();
                for language in Language::ALL {
                    par.language_mut(language).production_lines = reference_lines(benchy, language);
                }
                Ok(par)
            }
        }
    }

    pub fn language(&self, language: Language) -> &LanguagePar {
        match language {
            Language::Rust => &self.rust,
            Language::Go => &self.go,
            Language::CSharp => &self.csharp,
        }
    }

    fn language_mut(&mut self, language: Language) -> &mut LanguagePar {
        match language {
            Language::Rust => &mut self.rust,
            Language::Go => &mut self.go,
            Language::CSharp => &mut self.csharp,
        }
    }

    pub fn production_lines(&self, language: Language) -> usize {
        self.language(language).production_lines
    }
}

/// Production lines of the reference solution of `language`, counted as a candidate's are.
pub fn reference_lines(benchy: &Path, language: Language) -> usize {
    let files = sources::production_files(&benchy.join("validation/reference"), language);
    sources::production_line_total(&files)
}

/// Writes `par.toml` from the reference line counts and the reference timings of this scoring run.
///
/// # Errors
///
/// Fails when the file cannot be written.
pub fn write(
    path: &Path,
    benchy: &Path,
    timings: &[(Language, ReferenceTimes)],
) -> Result<(), ScoreError> {
    let mut text = String::from(
        "# Par: the measurements of the reference solutions that the rubric scores against.\n\
         # Written by: benchy score --reference --par-out validation/par.toml\n\
         # Production lines set par for E.production_lines. The timings record the machine they were\n\
         # measured on; C compares each candidate with its reference in alternating runs of the same session.\n",
    );
    for language in Language::ALL {
        let _ = writeln!(text, "\n[{}]", language.as_str());
        let _ = writeln!(
            text,
            "production_lines = {}",
            reference_lines(benchy, language)
        );
        if let Some((_, times)) = timings.iter().find(|(l, _)| *l == language) {
            let _ = writeln!(text, "startup_seconds = {:.4}", times.startup_seconds);
            let _ = writeln!(text, "peak_memory_mb = {:.1}", times.peak_memory_mb);
            if let Some(exponent) = times.scaling_exponent {
                let _ = writeln!(text, "scaling_exponent = {exponent:.3}");
            }
            let _ = writeln!(text, "\n[{}.throughput_seconds]", language.as_str());
            for (name, seconds) in &times.throughput_seconds {
                let _ = writeln!(text, "{name} = {seconds:.3}");
            }
        }
    }
    fs::write(path, text).map_err(ScoreError::io(format!("write {}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_par_matches_the_reference_line_counts() {
        let benchy = Path::new(env!("CARGO_MANIFEST_DIR"));

        let par = Par::load_or_measure(benchy).expect("par.toml parses");

        for language in Language::ALL {
            assert_eq!(
                par.production_lines(language),
                reference_lines(benchy, language),
                "par.toml is stale for {}; write it again with benchy score --reference --par-out validation/par.toml",
                language.as_str()
            );
        }
    }
}
