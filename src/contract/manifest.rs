//! The harness manifest: what a harness under test is, declared in a `harnesses/*.toml` file.
//! Two scorecards compare only when their manifests agree on the comparability keys.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The one fixed sentence that starts every run. A manifest's entry command may wrap it, never change it.
pub const KICKOFF_PROMPT: &str = "Read TASK.md and complete the task.";

/// The only replies an operator may give an interactive run. Each one given is counted.
pub const PERMITTED_REPLIES: [&str; 2] = [
    "Use your best judgement and TASK.md.",
    "Approved, continue.",
];

/// The placeholder in [`HarnessManifest::entry`] that the kickoff prompt replaces.
pub const PROMPT_PLACEHOLDER: &str = "{prompt}";

/// The manifest format version this build reads.
pub const MANIFEST_SCHEMA: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("failed to read harness manifest {path}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse harness manifest")]
    Parse(#[from] toml::de::Error),
    #[error("manifest schema is {found}, this build reads schema {MANIFEST_SCHEMA}")]
    Schema { found: u32 },
    #[error("entry command {entry:?} must hold {PROMPT_PLACEHOLDER} exactly once")]
    Entry { entry: String },
    #[error("label {label:?} must be 1 to 40 characters of lowercase letters, digits and hyphens")]
    Label { label: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HarnessManifest {
    pub schema: u32,
    /// Short name used in `results/<label>/`.
    pub label: String,
    pub description: String,
    /// The first message of the run, with [`PROMPT_PLACEHOLDER`] where the kickoff prompt goes.
    pub entry: String,
    pub launch: Launch,
    pub permission_mode: String,
    pub auth: AuthMode,
    pub isolation: IsolationLevel,
    pub claude_code: ClaudeCode,
    /// Claude Code settings file copied into the run's configuration directory, relative to the manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_file: Option<String>,
    #[serde(default, rename = "plugin", skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<Plugin>,
    #[serde(default, rename = "tool_server", skip_serializing_if = "Vec::is_empty")]
    pub tool_servers: Vec<ToolServer>,
    #[serde(
        default,
        rename = "external_spend",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub external_spend: Vec<ExternalSpend>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Launch {
    /// Benchy starts `claude -p` and waits for it to exit.
    Headless,
    /// An operator starts the harness and runs `benchy finish` when it reports done.
    Interactive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthMode {
    ApiKey,
    Subscription,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IsolationLevel {
    CleanConfig,
    Container,
    HostConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaudeCode {
    pub version: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plugin {
    /// Plugin id as Claude Code names it, such as `name@marketplace`.
    pub id: String,
    /// Where the marketplace comes from, such as `github:owner/repo`.
    pub source: String,
    pub commit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolServer {
    pub name: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
}

/// Spend that leaves no Claude Code transcript, entered by hand and flagged in the report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalSpend {
    pub description: String,
    pub usd: f64,
}

/// The fields that must match for two runs to compare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparabilityKey {
    pub claude_code_version: String,
    pub model: String,
    pub auth: AuthMode,
    pub isolation: IsolationLevel,
}

impl HarnessManifest {
    /// Reads and checks a manifest file.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be read, does not parse, or breaks a rule of [`HarnessManifest::from_toml`].
    pub fn load(path: &Path) -> Result<HarnessManifest, ManifestError> {
        let text = std::fs::read_to_string(path).map_err(|source| ManifestError::Read {
            path: path.display().to_string(),
            source,
        })?;
        HarnessManifest::from_toml(&text)
    }

    /// Parses a manifest and checks its schema, label and entry command.
    ///
    /// # Errors
    ///
    /// Fails on bad TOML, an unknown schema, a label that is not a plain slug,
    /// or an entry command without exactly one prompt placeholder.
    pub fn from_toml(text: &str) -> Result<HarnessManifest, ManifestError> {
        let manifest: HarnessManifest = toml::from_str(text)?;
        if manifest.schema != MANIFEST_SCHEMA {
            return Err(ManifestError::Schema {
                found: manifest.schema,
            });
        }
        let label_is_slug = (1..=40).contains(&manifest.label.len())
            && manifest
                .label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !label_is_slug {
            return Err(ManifestError::Label {
                label: manifest.label,
            });
        }
        if manifest.entry.matches(PROMPT_PLACEHOLDER).count() != 1 {
            return Err(ManifestError::Entry {
                entry: manifest.entry,
            });
        }
        Ok(manifest)
    }

    /// The first message of the run: the entry command with the kickoff prompt in place.
    pub fn kickoff_message(&self) -> String {
        self.entry.replace(PROMPT_PLACEHOLDER, KICKOFF_PROMPT)
    }

    pub fn comparability_key(&self) -> ComparabilityKey {
        ComparabilityKey {
            claude_code_version: self.claude_code.version.clone(),
            model: self.claude_code.model.clone(),
            auth: self.auth,
            isolation: self.isolation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn harness(name: &str) -> HarnessManifest {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("harnesses")
            .join(name);
        HarnessManifest::load(&path).expect("harness manifest is valid")
    }

    #[test]
    fn repo_manifests_round_trip_through_toml() {
        for name in ["claude-code.toml", "super-rex.toml"] {
            let manifest = harness(name);
            let text = toml::to_string(&manifest).expect("a manifest serialises");
            assert_eq!(
                HarnessManifest::from_toml(&text).expect("it parses again"),
                manifest
            );
        }
    }

    #[test]
    fn the_control_sends_the_bare_kickoff_prompt() {
        let control = harness("claude-code.toml");
        assert_eq!(control.kickoff_message(), KICKOFF_PROMPT);
        assert!(control.plugins.is_empty());
    }

    #[test]
    fn the_example_plugin_harness_compares_with_the_control() {
        let control = harness("claude-code.toml");
        let plugin = harness("super-rex.toml");
        assert_eq!(plugin.comparability_key(), control.comparability_key());
        assert_eq!(plugin.plugins.len(), 1);
    }

    #[test]
    fn an_entry_without_the_placeholder_is_rejected() {
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("harnesses/claude-code.toml"),
        )
        .expect("manifest is readable");
        let broken = text.replace("entry = \"{prompt}\"", "entry = \"/go\"");
        assert!(matches!(
            HarnessManifest::from_toml(&broken),
            Err(ManifestError::Entry { .. })
        ));
    }
}
