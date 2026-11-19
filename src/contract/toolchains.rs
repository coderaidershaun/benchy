//! Pinned compiler and analyser versions with their container images, read from `validation/toolchains.toml`.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ToolchainsError {
    #[error("failed to read toolchains {path}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse toolchains")]
    Parse(#[from] toml::de::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Toolchains {
    pub version: String,
    pub rust: Toolchain,
    pub go: Toolchain,
    pub dotnet: DotnetToolchain,
    pub staticcheck: Staticcheck,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Toolchain {
    pub version: String,
    pub image: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DotnetToolchain {
    pub version: String,
    pub target_framework: String,
    pub image: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Staticcheck {
    pub version: String,
    pub module_version: String,
    pub install: String,
}

impl Toolchains {
    /// # Errors
    ///
    /// Fails when the file cannot be read or does not parse.
    pub fn load(path: &Path) -> Result<Toolchains, ToolchainsError> {
        let text = std::fs::read_to_string(path).map_err(|source| ToolchainsError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Ok(toml::from_str(&text)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_brief_names_the_pinned_versions_and_images() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let toolchains =
            Toolchains::load(&root.join("validation/toolchains.toml")).expect("valid toolchains");
        let brief =
            std::fs::read_to_string(root.join("task/TASK.md")).expect("TASK.md is readable");
        let pinned = [
            &toolchains.rust.version,
            &toolchains.rust.image,
            &toolchains.go.version,
            &toolchains.go.image,
            &toolchains.dotnet.version,
            &toolchains.dotnet.target_framework,
            &toolchains.dotnet.image,
            &toolchains.staticcheck.version,
            &toolchains.staticcheck.module_version,
            &toolchains.staticcheck.install,
        ];
        for value in pinned {
            assert!(
                brief.contains(value.as_str()),
                "TASK.md does not name {value}"
            );
        }
    }
}
