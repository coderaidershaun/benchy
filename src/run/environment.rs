//! The environment the harness runs with: a short allow-list of the operator's variables plus the run's configuration.
//! Everything else is dropped, because variables such as a parent Claude session's ids or a PATH into the benchy checkout would leak.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use benchy::contract::AuthMode;

use super::layout::RunDir;

/// Variables passed through from the operator's environment when they are set.
const PASSED_THROUGH: [&str; 16] = [
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "TERM",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TMPDIR",
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "NO_PROXY",
    "https_proxy",
    "http_proxy",
    "no_proxy",
    "SSL_CERT_FILE",
];
const API_KEY: &str = "ANTHROPIC_API_KEY";
/// A long-lived subscription token from `claude setup-token`. A relocated configuration directory has no login of
/// its own, so a subscription run needs this variable.
pub const OAUTH_TOKEN: &str = "CLAUDE_CODE_OAUTH_TOKEN";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessEnvironment {
    pub workspace: PathBuf,
    pub config_dir: PathBuf,
    pub variables: BTreeMap<String, String>,
}

impl HarnessEnvironment {
    /// Builds the environment from `operator`, the variables of the process that runs benchy.
    pub fn new(
        run: &RunDir,
        auth: AuthMode,
        benchy_root: &Path,
        operator: impl Iterator<Item = (String, String)>,
    ) -> HarnessEnvironment {
        let operator: BTreeMap<String, String> = operator.collect();
        let mut variables: BTreeMap<String, String> = PASSED_THROUGH
            .iter()
            .filter_map(|name| operator.get(*name).map(|v| (name.to_string(), v.clone())))
            .collect();
        let secret = match auth {
            AuthMode::ApiKey => API_KEY,
            AuthMode::Subscription => OAUTH_TOKEN,
        };
        if let Some(value) = operator.get(secret) {
            variables.insert(secret.to_string(), value.clone());
        }
        let path = operator
            .get("PATH")
            .map(String::as_str)
            .unwrap_or("/usr/bin:/bin");
        let path: Vec<&str> = path
            .split(':')
            .filter(|dir| !dir.is_empty() && !Path::new(dir).starts_with(benchy_root))
            .collect();
        variables.insert("PATH".to_string(), path.join(":"));
        variables.insert(
            "CLAUDE_CONFIG_DIR".to_string(),
            run.config().display().to_string(),
        );
        variables.insert("DISABLE_AUTOUPDATER".to_string(), "1".to_string());
        // The operator's global git settings would name them and could point at paths outside the run.
        variables.insert(
            "GIT_CONFIG_GLOBAL".to_string(),
            run.config().join("gitconfig").display().to_string(),
        );
        variables.insert("GIT_CONFIG_NOSYSTEM".to_string(), "1".to_string());
        variables.insert("PWD".to_string(), run.workspace().display().to_string());
        HarnessEnvironment {
            workspace: run.workspace(),
            config_dir: run.config(),
            variables,
        }
    }

    /// Whether the variable that logs the run in is set: the API key or the subscription token.
    pub fn has_login(&self) -> bool {
        self.variables.contains_key(API_KEY) || self.variables.contains_key(OAUTH_TOKEN)
    }

    /// The shell line an operator pastes to start the harness by hand. Secrets are shown by name, never by value.
    pub fn shell_line(&self, arguments: &[String]) -> String {
        let mut parts = vec![
            "cd".to_string(),
            quote(&self.workspace.display().to_string()),
            "&&".to_string(),
            "env -i".to_string(),
        ];
        for (name, value) in &self.variables {
            if name == API_KEY || name == OAUTH_TOKEN {
                parts.push(format!("{name}=\"${name}\""));
            } else {
                parts.push(format!("{name}={}", quote(value)));
            }
        }
        parts.push("claude".to_string());
        parts.extend(arguments.iter().map(|a| quote(a)));
        parts.join(" ")
    }
}

fn quote(text: &str) -> String {
    let is_plain = !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./:=@%+,".contains(&b));
    if is_plain {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parent_claude_session_and_the_benchy_checkout_do_not_reach_the_harness() {
        let run = RunDir {
            dir: PathBuf::from("/runs/abc123"),
        };
        let operator = [
            ("HOME", "/home/u"),
            ("PATH", "/benchy/target/debug:/usr/bin"),
            ("CLAUDECODE", "1"),
            ("CLAUDE_CODE_SESSION_ID", "parent"),
            ("ANTHROPIC_API_KEY", "secret"),
            ("CLAUDE_CODE_OAUTH_TOKEN", "token"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()));
        let environment = HarnessEnvironment::new(
            &run,
            AuthMode::Subscription,
            Path::new("/benchy"),
            operator.into_iter(),
        );
        assert_eq!(environment.variables["PATH"], "/usr/bin");
        assert_eq!(
            environment.variables["CLAUDE_CONFIG_DIR"],
            "/runs/abc123/config"
        );
        assert!(!environment.variables.contains_key("CLAUDECODE"));
        assert!(!environment.variables.contains_key("CLAUDE_CODE_SESSION_ID"));
        assert!(
            !environment.variables.contains_key("ANTHROPIC_API_KEY"),
            "subscription runs get no API key"
        );
        assert_eq!(environment.variables["CLAUDE_CODE_OAUTH_TOKEN"], "token");
        assert_eq!(
            environment.variables["GIT_CONFIG_GLOBAL"],
            "/runs/abc123/config/gitconfig"
        );
        let line = environment.shell_line(&[]);
        assert!(
            !line.contains("secret") && !line.contains("=token"),
            "{line}"
        );
    }
}
