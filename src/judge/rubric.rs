//! The judge's anchored rubric and fixed prompt, read from `validation/judge/`, and the prompt they make together.

use std::fs;
use std::path::Path;

use benchy::contract::{Language, Rubric, Source};
use serde::Deserialize;

use super::runner::JudgeError;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JudgeRubric {
    pub version: String,
    #[serde(rename = "item")]
    pub items: Vec<RubricItem>,
    /// The prompt template, from `prompt.md`.
    #[serde(skip)]
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RubricItem {
    /// Metric key as `validation/rubric.toml` keys it, such as `G.self_evidence`.
    pub key: String,
    pub name: String,
    pub question: String,
    pub guides: String,
    /// One anchor for each level, from 0 to 4.
    pub anchors: Vec<String>,
}

impl JudgeRubric {
    /// Reads `rubric.toml` and `prompt.md` from `dir` and checks them against the scoring rubric.
    ///
    /// # Errors
    ///
    /// Fails when a file is unreadable, or the items differ from the judged metrics, or an item lacks an anchor per level.
    pub fn load(dir: &Path, scoring: &Rubric) -> Result<JudgeRubric, JudgeError> {
        let read = |name: &str| {
            let path = dir.join(name);
            fs::read_to_string(&path).map_err(|source| JudgeError::Read {
                path: path.display().to_string(),
                source,
            })
        };
        let mut rubric: JudgeRubric = toml::from_str(&read("rubric.toml")?)
            .map_err(|e| JudgeError::Setup(format!("validation/judge/rubric.toml: {e}")))?;
        rubric.prompt = read("prompt.md")?;
        rubric.check(scoring)?;
        Ok(rubric)
    }

    fn check(&self, scoring: &Rubric) -> Result<(), JudgeError> {
        let mut judged: Vec<String> = scoring
            .categories
            .iter()
            .flat_map(|c| {
                c.metrics
                    .iter()
                    .filter(|m| m.source == Source::Judged)
                    .map(move |m| format!("{}.{}", c.id.as_str(), m.id))
            })
            .collect();
        let mut items: Vec<String> = self.items.iter().map(|i| i.key.clone()).collect();
        judged.sort();
        items.sort();
        if judged != items {
            return Err(JudgeError::Setup(format!(
                "judge items {items:?} differ from the judged metrics {judged:?}"
            )));
        }
        let levels = scoring.judge.scale_max as usize + 1;
        for item in &self.items {
            let numbered = item
                .anchors
                .iter()
                .enumerate()
                .all(|(level, anchor)| anchor.starts_with(&format!("{level}: ")));
            if item.anchors.len() != levels || !numbered {
                return Err(JudgeError::Setup(format!(
                    "{} needs {levels} anchors numbered 0 to {}",
                    item.key, scoring.judge.scale_max
                )));
            }
        }
        Ok(())
    }

    /// The full first prompt for these languages.
    pub fn prompt_for(&self, languages: &[Language]) -> String {
        let mut rubric = String::new();
        for item in &self.items {
            rubric.push_str(&format!(
                "## {} ({})\n\n{}\nSources: {}\n\n",
                item.key, item.name, item.question, item.guides
            ));
            for anchor in &item.anchors {
                rubric.push_str(&format!("- {anchor}\n"));
            }
            rubric.push('\n');
        }
        let names: Vec<&str> = languages.iter().map(|l| l.as_str()).collect();
        self.prompt
            .replace("{languages}", &names.join(", "))
            .replace("{rubric}", rubric.trim_end())
            .replace("{count}", &(languages.len() * self.items.len()).to_string())
    }

    /// FNV-1a over the prompt template and every anchor, so any edit changes it.
    pub fn hash(&self) -> String {
        let mut text = self.prompt.clone();
        for item in &self.items {
            text.push_str(&item.key);
            text.push_str(&item.question);
            text.push_str(&item.anchors.concat());
        }
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in text.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        format!("{hash:016x}")
    }
}
