//! Builds the run's clean Claude Code configuration directory from the harness manifest, and the command that starts it.
//! Nothing comes from the operator's own configuration: no memory, no transcripts, no plugins the manifest does not name.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use benchy::contract::{HarnessManifest, Plugin};
use serde_json::{Map, Value, json};

use super::RunError;
use super::environment::HarnessEnvironment;

/// The harness's global git settings: a neutral author and nothing else.
const NEUTRAL_GITCONFIG: &str =
    "[user]\n\tname = runner\n\temail = runner@localhost\n[init]\n\tdefaultBranch = main\n";

/// Writes `settings.json`, `gitconfig` and, when the manifest names tool servers, `mcp.json`.
///
/// # Errors
///
/// Fails when the manifest's settings file is unreadable or not a JSON object, a plugin source is not supported,
/// or a file cannot be written.
pub fn write_config(
    config: &Path,
    manifest: &HarnessManifest,
    manifest_dir: &Path,
) -> Result<(), RunError> {
    create_dir(config)?;
    let mut settings = match &manifest.settings_file {
        Some(file) => read_settings(&manifest_dir.join(file))?,
        None => Map::new(),
    };
    if !manifest.plugins.is_empty() {
        let mut marketplaces = Map::new();
        let mut enabled = Map::new();
        for plugin in &manifest.plugins {
            marketplaces.insert(
                marketplace_name(plugin)?.to_string(),
                marketplace_source(plugin)?,
            );
            enabled.insert(plugin.id.clone(), Value::Bool(true));
        }
        settings.insert(
            "extraKnownMarketplaces".to_string(),
            Value::Object(marketplaces),
        );
        settings.insert("enabledPlugins".to_string(), Value::Object(enabled));
    }
    write_text(&config.join("settings.json"), &Value::Object(settings))?;
    fs::write(config.join("gitconfig"), NEUTRAL_GITCONFIG).map_err(|source| RunError::Io {
        action: format!("write {}", config.join("gitconfig").display()),
        source,
    })?;
    if !manifest.tool_servers.is_empty() {
        let servers: Map<String, Value> = manifest
            .tool_servers
            .iter()
            .map(|s| {
                (
                    s.name.clone(),
                    json!({ "command": s.command, "args": s.args }),
                )
            })
            .collect();
        write_text(&mcp_config(config), &json!({ "mcpServers": servers }))?;
    }
    Ok(())
}

/// Installs each plugin of the manifest into the run's configuration and checks that the installed commit is the declared one.
///
/// # Errors
///
/// Fails when `claude plugin` fails or the installed commit differs from the manifest.
pub fn install_plugins(
    manifest: &HarnessManifest,
    environment: &HarnessEnvironment,
) -> Result<(), RunError> {
    for plugin in &manifest.plugins {
        let repo = github_repo(plugin)?;
        run_claude(environment, &["plugin", "marketplace", "add", repo])?;
        run_claude(environment, &["plugin", "install", &plugin.id])?;
        let installed = installed_commit(&environment.config_dir, &plugin.id)?;
        if installed.as_deref() != Some(plugin.commit.as_str()) {
            return Err(RunError::PluginCommit {
                plugin: plugin.id.clone(),
                declared: plugin.commit.clone(),
                installed: installed.unwrap_or_else(|| "nothing".to_string()),
            });
        }
    }
    Ok(())
}

/// Arguments every launch of the harness takes, headless or interactive.
pub fn claude_arguments(manifest: &HarnessManifest, config: &Path) -> Vec<String> {
    let mut arguments = vec![
        "--model".to_string(),
        manifest.claude_code.model.clone(),
        "--permission-mode".to_string(),
        manifest.permission_mode.clone(),
    ];
    if let Some(effort) = &manifest.claude_code.effort {
        arguments.extend(["--effort".to_string(), effort.clone()]);
    }
    if !manifest.tool_servers.is_empty() {
        arguments.extend([
            "--mcp-config".to_string(),
            mcp_config(config).display().to_string(),
        ]);
    }
    arguments
}

fn mcp_config(config: &Path) -> PathBuf {
    config.join("mcp.json")
}

fn marketplace_name(plugin: &Plugin) -> Result<&str, RunError> {
    plugin
        .id
        .split_once('@')
        .map(|(_, marketplace)| marketplace)
        .ok_or_else(|| RunError::PluginSource {
            plugin: plugin.id.clone(),
            why: "the id must be name@marketplace".to_string(),
        })
}

fn github_repo(plugin: &Plugin) -> Result<&str, RunError> {
    plugin
        .source
        .strip_prefix("github:")
        .ok_or_else(|| RunError::PluginSource {
            plugin: plugin.id.clone(),
            why: format!(
                "source {} is not github:owner/repo, the only source benchy installs",
                plugin.source
            ),
        })
}

fn marketplace_source(plugin: &Plugin) -> Result<Value, RunError> {
    Ok(json!({ "source": { "source": "github", "repo": github_repo(plugin)? } }))
}

fn installed_commit(config: &Path, plugin_id: &str) -> Result<Option<String>, RunError> {
    let path = config.join("plugins/installed_plugins.json");
    let text = fs::read_to_string(&path).map_err(|source| RunError::Io {
        action: format!("read {}", path.display()),
        source,
    })?;
    let installed: Value = serde_json::from_str(&text).map_err(|source| RunError::Json {
        path: path.display().to_string(),
        source,
    })?;
    let commit = installed
        .get("plugins")
        .and_then(|p| p.get(plugin_id))
        .and_then(Value::as_array)
        .and_then(|installs| installs.first())
        .and_then(|install| install.get("gitCommitSha"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(commit)
}

fn run_claude(environment: &HarnessEnvironment, arguments: &[&str]) -> Result<(), RunError> {
    let shown = format!("claude {}", arguments.join(" "));
    let status = Command::new("claude")
        .args(arguments)
        .env_clear()
        .envs(&environment.variables)
        .current_dir(&environment.workspace)
        .status()
        .map_err(|source| RunError::Io {
            action: format!("start {shown}"),
            source,
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(RunError::CommandFailed {
            command: shown,
            status: status.to_string(),
        })
    }
}

fn read_settings(path: &Path) -> Result<Map<String, Value>, RunError> {
    let text = fs::read_to_string(path).map_err(|source| RunError::Io {
        action: format!("read settings file {}", path.display()),
        source,
    })?;
    match serde_json::from_str(&text) {
        Ok(Value::Object(settings)) => Ok(settings),
        Ok(_) => Err(RunError::SettingsNotObject {
            path: path.display().to_string(),
        }),
        Err(source) => Err(RunError::Json {
            path: path.display().to_string(),
            source,
        }),
    }
}

fn write_text(path: &Path, value: &Value) -> Result<(), RunError> {
    let text = serde_json::to_string_pretty(value).expect("a JSON value always serialises");
    fs::write(path, text + "\n").map_err(|source| RunError::Io {
        action: format!("write {}", path.display()),
        source,
    })
}

pub fn create_dir(path: &Path) -> Result<(), RunError> {
    fs::create_dir_all(path).map_err(|source| RunError::Io {
        action: format!("create directory {}", path.display()),
        source,
    })
}
