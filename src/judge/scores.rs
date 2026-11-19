//! What `benchy judge` writes as `judge.json`: a level for each judged item of each language, from several judgements.

use benchy::contract::Language;
use serde::{Deserialize, Serialize};

/// The file name of the judged scores inside the operator data of a run.
pub const JUDGED_FILE: &str = "judge.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JudgedScores {
    pub model: String,
    /// Hash of the prompt and the anchored rubric, so a change to either shows in every judged run.
    pub prompt_hash: String,
    pub samples: u32,
    pub items: Vec<JudgedItem>,
}

/// One item, such as `G.self_evidence`, for one language.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JudgedItem {
    pub language: Language,
    /// Metric key as the rubric keys it.
    pub key: String,
    /// The level each independent judgement gave, in the order they ran.
    pub levels: Vec<u32>,
    /// The median level: the one that counts.
    pub median: u32,
    /// Highest level minus lowest level.
    pub spread: u32,
    /// `file:line` citations of the judgement that gave the median.
    pub evidence: Vec<String>,
    pub reason: String,
}

impl JudgedScores {
    pub fn item(&self, language: Language, key: &str) -> Option<&JudgedItem> {
        self.items
            .iter()
            .find(|i| i.language == language && i.key == key)
    }
}
