//! Reads one judgement from the judge's reply and rejects it unless every level is in range and cites real lines.

use std::path::Path;

use benchy::contract::Language;
use serde::Deserialize;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AnswerError {
    #[error("the reply holds no JSON object")]
    NoJson,
    #[error("the reply's JSON does not have the required form: {0}")]
    Form(String),
    #[error("there is no judgement for {language} {item}")]
    Missing { language: String, item: String },
    #[error("there are two judgements for {language} {item}")]
    Duplicate { language: String, item: String },
    #[error("{language} {item} has level {level}, but levels run from 0 to {max}")]
    Level {
        language: String,
        item: String,
        level: u32,
        max: u32,
    },
    #[error("{language} {item} cites no evidence")]
    NoEvidence { language: String, item: String },
    #[error(
        "{language} {item} cites {citation}, which is not a path:line of a file that exists with that line"
    )]
    BadCitation {
        language: String,
        item: String,
        citation: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Answer {
    pub judgements: Vec<Judgement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Judgement {
    pub language: Language,
    pub item: String,
    pub level: u32,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub reason: String,
}

/// Parses the judge's text and checks it covers each language and item once, with real evidence.
pub fn parse(
    text: &str,
    languages: &[Language],
    items: &[String],
    max: u32,
    workspace: &Path,
) -> Result<Answer, AnswerError> {
    let start = text.find('{').ok_or(AnswerError::NoJson)?;
    let end = text.rfind('}').ok_or(AnswerError::NoJson)?;
    let answer: Answer =
        serde_json::from_str(&text[start..=end]).map_err(|e| AnswerError::Form(e.to_string()))?;
    for &language in languages {
        for item in items {
            let found: Vec<&Judgement> = answer
                .judgements
                .iter()
                .filter(|j| j.language == language && &j.item == item)
                .collect();
            let names = || (language.as_str().to_string(), item.clone());
            let judgement = match found.as_slice() {
                [one] => *one,
                [] => {
                    let (language, item) = names();
                    return Err(AnswerError::Missing { language, item });
                }
                _ => {
                    let (language, item) = names();
                    return Err(AnswerError::Duplicate { language, item });
                }
            };
            if judgement.level > max {
                let (language, item) = names();
                return Err(AnswerError::Level {
                    language,
                    item,
                    level: judgement.level,
                    max,
                });
            }
            if judgement.evidence.is_empty() {
                let (language, item) = names();
                return Err(AnswerError::NoEvidence { language, item });
            }
            if let Some(citation) = judgement
                .evidence
                .iter()
                .find(|c| !cites_a_real_line(c, workspace))
            {
                let (language, item) = names();
                return Err(AnswerError::BadCitation {
                    language,
                    item,
                    citation: citation.clone(),
                });
            }
        }
    }
    Ok(answer)
}

fn cites_a_real_line(citation: &str, workspace: &Path) -> bool {
    let Some((path, line)) = citation.rsplit_once(':') else {
        return false;
    };
    let Ok(line) = line.parse::<usize>() else {
        return false;
    };
    let relative = Path::new(path);
    let stays_inside = relative.is_relative()
        && relative
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)));
    if !stays_inside || line == 0 {
        return false;
    }
    std::fs::read_to_string(workspace.join(relative)).is_ok_and(|text| text.lines().count() >= line)
}
