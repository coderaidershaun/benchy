//! The judge: scores what cannot be measured, through `claude -p` with a fixed model, prompt and anchored rubric.
//! It sees the committed code and `DELIVERY.md` only, and every level it gives must cite `file:line` evidence.

mod answer;
pub mod command;
mod rubric;
mod runner;
mod scores;

pub use rubric::JudgeRubric;
pub use runner::Judge;
#[cfg(test)]
pub use runner::JudgeError;
pub use scores::{JUDGED_FILE, JudgedItem, JudgedScores};

#[cfg(test)]
mod tests;
