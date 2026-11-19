//! Cost, time and agents entered by hand, for runs whose transcripts the collector cannot read or does not trust.
//! Every value entered is named in the telemetry so the report can flag it.

use std::path::Path;

use benchy::contract::{Agent, Telemetry, TelemetrySource};
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum ManualError {
    #[error("failed to read manual entry {path}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse manual entry")]
    Parse(#[from] toml::de::Error),
    #[error(
        "manual entry sets nothing; give at least one of cost_usd, busy_seconds, wall_seconds, agent_seconds or [[agent]]"
    )]
    Nothing,
    #[error(
        "the transcripts could not be read, so the manual entry must give {missing}; add `{missing} = <number>` to the file"
    )]
    Missing { missing: &'static str },
}

/// The file an operator writes, such as `.runs/<run-id>/manual.toml`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualEntry {
    pub cost_usd: Option<f64>,
    pub busy_seconds: Option<f64>,
    pub wall_seconds: Option<f64>,
    pub agent_seconds: Option<f64>,
    #[serde(default, rename = "agent")]
    pub agents: Vec<Agent>,
}

impl ManualEntry {
    /// # Errors
    ///
    /// Fails when the file cannot be read or does not parse.
    pub fn load(path: &Path) -> Result<ManualEntry, ManualError> {
        let text = std::fs::read_to_string(path).map_err(|source| ManualError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Ok(toml::from_str(&text)?)
    }

    /// Puts the entered values over the collected telemetry, or builds telemetry from them alone when there is none.
    ///
    /// # Errors
    ///
    /// Fails when the entry sets nothing, or when there is no collected telemetry and cost or busy time is missing.
    pub fn apply(&self, collected: Option<Telemetry>) -> Result<Telemetry, ManualError> {
        let fields = self.fields_set();
        if fields.is_empty() {
            return Err(ManualError::Nothing);
        }
        let mut telemetry = match collected {
            Some(collected) => Telemetry {
                source: TelemetrySource::Mixed,
                ..collected
            },
            None => {
                if self.cost_usd.is_none() {
                    return Err(ManualError::Missing {
                        missing: "cost_usd",
                    });
                }
                if self.busy_seconds.is_none() {
                    return Err(ManualError::Missing {
                        missing: "busy_seconds",
                    });
                }
                Telemetry {
                    source: TelemetrySource::Manual,
                    ..Telemetry::default()
                }
            }
        };
        if let Some(cost) = self.cost_usd {
            telemetry.cost_usd = cost;
        }
        if let Some(busy) = self.busy_seconds {
            telemetry.time.busy_seconds = busy;
        }
        if let Some(wall) = self.wall_seconds {
            telemetry.time.wall_seconds = wall;
        }
        if let Some(agent_seconds) = self.agent_seconds {
            telemetry.time.agent_seconds = agent_seconds;
        }
        if !self.agents.is_empty() {
            telemetry.agents.clone_from(&self.agents);
            let count = |main: bool| {
                let n = self
                    .agents
                    .iter()
                    .filter(|a| (a.depth == 0) == main)
                    .count();
                u32::try_from(n).unwrap_or(u32::MAX)
            };
            telemetry.activity.sessions = count(true);
            telemetry.activity.subagents = count(false);
            telemetry.activity.max_depth = self.agents.iter().map(|a| a.depth).max().unwrap_or(0);
        }
        for field in fields {
            if !telemetry.manual_entries.iter().any(|f| f == field) {
                telemetry.manual_entries.push(field.to_string());
            }
        }
        Ok(telemetry)
    }

    fn fields_set(&self) -> Vec<&'static str> {
        let named = [
            ("cost_usd", self.cost_usd.is_some()),
            ("time.busy_seconds", self.busy_seconds.is_some()),
            ("time.wall_seconds", self.wall_seconds.is_some()),
            ("time.agent_seconds", self.agent_seconds.is_some()),
            ("agents", !self.agents.is_empty()),
        ];
        named
            .into_iter()
            .filter(|&(_, set)| set)
            .map(|(name, _)| name)
            .collect()
    }
}
