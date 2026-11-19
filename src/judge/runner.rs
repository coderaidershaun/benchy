//! Runs the independent judgements, asks again after an invalid reply, and keeps the median of each item.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use benchy::contract::Language;
use serde_json::Value;

use super::answer::{self, Answer};
use super::rubric::JudgeRubric;
use super::scores::{JudgedItem, JudgedScores};

#[derive(Debug, thiserror::Error)]
pub enum JudgeError {
    #[error("failed to read {path}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("the judge is not set up: {0}")]
    Setup(String),
    #[error("failed to start the judge program {program}")]
    Start {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "judgement {sample} gave no valid answer in {attempts} attempts; the last was rejected because {last}"
    )]
    GaveUp {
        sample: u32,
        attempts: u32,
        last: String,
    },
}

/// The fixed settings of a judge: the program to call, the model, the number of judgements and the retry bound.
#[derive(Debug, Clone)]
pub struct Judge {
    /// `claude`, or a stand-in program in tests.
    pub program: PathBuf,
    pub model: String,
    pub samples: u32,
    pub max_attempts: u32,
    pub scale_max: u32,
    pub rubric: JudgeRubric,
}

impl Judge {
    /// # Errors
    ///
    /// Fails when the program cannot start, or one judgement stays invalid after every attempt.
    pub fn judge(
        &self,
        workspace: &Path,
        languages: &[Language],
    ) -> Result<JudgedScores, JudgeError> {
        let items: Vec<String> = self.rubric.items.iter().map(|i| i.key.clone()).collect();
        let first_prompt = self.rubric.prompt_for(languages);
        let mut answers = Vec::new();
        for sample in 1..=self.samples {
            answers.push(self.one_judgement(
                sample,
                &first_prompt,
                workspace,
                languages,
                &items,
            )?);
        }
        let mut judged = Vec::new();
        for &language in languages {
            for key in &items {
                judged.push(median_item(&answers, language, key));
            }
        }
        Ok(JudgedScores {
            model: self.model.clone(),
            prompt_hash: self.rubric.hash(),
            samples: self.samples,
            items: judged,
        })
    }

    fn one_judgement(
        &self,
        sample: u32,
        first_prompt: &str,
        workspace: &Path,
        languages: &[Language],
        items: &[String],
    ) -> Result<Answer, JudgeError> {
        let mut prompt = first_prompt.to_string();
        let mut last = String::new();
        for _ in 0..self.max_attempts {
            let text = self.ask(&prompt, workspace)?;
            match answer::parse(&text, languages, items, self.scale_max, workspace) {
                Ok(answer) => return Ok(answer),
                Err(error) => {
                    last = error.to_string();
                    prompt = format!(
                        "{first_prompt}\n\nYour previous answer was rejected because {last}. \
                         Answer again with one JSON object in the required form."
                    );
                }
            }
        }
        Err(JudgeError::GaveUp {
            sample,
            attempts: self.max_attempts,
            last,
        })
    }

    /// One call of `claude -p`. The reply text is the `result` field of its JSON output.
    fn ask(&self, prompt: &str, workspace: &Path) -> Result<String, JudgeError> {
        let output = Command::new(&self.program)
            .args([
                "-p",
                prompt,
                "--model",
                &self.model,
                "--output-format",
                "json",
            ])
            .args([
                "--allowedTools",
                "Read,Glob,Grep",
                "--permission-mode",
                "default",
            ])
            .current_dir(workspace)
            .stdin(Stdio::null())
            .output()
            .map_err(|source| JudgeError::Start {
                program: self.program.display().to_string(),
                source,
            })?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let result = serde_json::from_str::<Value>(&stdout)
            .ok()
            .and_then(|v| v.get("result").and_then(Value::as_str).map(str::to_string));
        Ok(result.unwrap_or(stdout))
    }
}

/// The median level over the judgements, with the evidence of the first judgement that gave it.
fn median_item(answers: &[Answer], language: Language, key: &str) -> JudgedItem {
    let picks: Vec<&answer::Judgement> = answers
        .iter()
        .filter_map(|a| {
            a.judgements
                .iter()
                .find(|j| j.language == language && j.item == key)
        })
        .collect();
    let levels: Vec<u32> = picks.iter().map(|j| j.level).collect();
    let mut sorted = levels.clone();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2];
    let chosen = picks
        .iter()
        .find(|j| j.level == median)
        .expect("the median is one of the levels");
    JudgedItem {
        language,
        key: key.to_string(),
        spread: sorted[sorted.len() - 1] - sorted[0],
        median,
        levels,
        evidence: chosen.evidence.clone(),
        reason: chosen.reason.clone(),
    }
}
