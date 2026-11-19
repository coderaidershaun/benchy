//! Creates a run: the workspace, the clean configuration, the operator's record, and a check that nothing points back to benchy.

use std::fs;
use std::path::{Path, PathBuf};

use benchy::contract::HarnessManifest;

use super::RunError;
use super::config::{self, create_dir};
use super::environment::HarnessEnvironment;
use super::layout::{self, OperatorDir, RunDir, RunId};
use super::record::RunRecord;
use super::workspace;
use crate::collect::timestamp;

/// What a new run needs to know. `task_pack` replaces `<benchy>/task` for smoke tests of the lifecycle itself.
#[derive(Debug, Clone)]
pub struct NewRun {
    pub manifest_path: PathBuf,
    pub benchy_root: PathBuf,
    pub run_root: PathBuf,
    pub task_pack: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct CreatedRun {
    pub id: RunId,
    pub run: RunDir,
    pub operator: OperatorDir,
    pub manifest: HarnessManifest,
    pub environment: HarnessEnvironment,
    pub record: RunRecord,
}

impl NewRun {
    /// # Errors
    ///
    /// Fails on a bad manifest, a run root inside the benchy checkout, a failed copy or git step, a failed plugin
    /// install, or when the finished run directory mentions the benchy checkout.
    pub fn create(&self) -> Result<CreatedRun, RunError> {
        let manifest = HarnessManifest::load(&self.manifest_path)?;
        layout::check_apart(&self.run_root, &self.benchy_root)?;
        let id = RunId::random()?;
        let run = RunDir::new(&self.run_root, &id);
        if run.dir.exists() {
            return Err(RunError::RunExists {
                path: run.dir.display().to_string(),
            });
        }
        let operator = OperatorDir::new(&self.benchy_root, &id);
        create_dir(&operator.dir)?;
        fs::copy(&self.manifest_path, operator.manifest()).map_err(|source| RunError::Io {
            action: format!(
                "copy {} into {}",
                self.manifest_path.display(),
                operator.dir.display()
            ),
            source,
        })?;
        let task_pack = self
            .task_pack
            .clone()
            .unwrap_or_else(|| self.benchy_root.join("task"));
        workspace::create(&task_pack, &run.workspace())?;
        let manifest_dir = self.manifest_path.parent().unwrap_or(Path::new("."));
        config::write_config(&run.config(), &manifest, manifest_dir)?;
        let environment =
            HarnessEnvironment::new(&run, manifest.auth, &self.benchy_root, std::env::vars());
        config::install_plugins(&manifest, &environment)?;
        check_no_leak(&run.dir, &self.benchy_root)?;
        let record = RunRecord {
            id: id.to_string(),
            label: manifest.label.clone(),
            run_dir: run.dir.clone(),
            task_pack,
            created: timestamp::format(timestamp::now()),
            launch: None,
            finish: None,
        };
        record.save(&operator.record())?;
        Ok(CreatedRun {
            id,
            run,
            operator,
            manifest,
            environment,
            record,
        })
    }
}

impl CreatedRun {
    /// What the operator needs to start an interactive harness by hand.
    pub fn instructions(&self) -> String {
        let arguments = config::claude_arguments(&self.manifest, &self.run.config());
        let mut lines = vec![
            format!("run id:      {}", self.id),
            format!("workspace:   {}", self.run.workspace().display()),
            format!("config:      {}", self.run.config().display()),
            format!("operator:    {}", self.operator.dir.display()),
            String::new(),
            "Start the harness with exactly this environment:".to_string(),
            format!("  {}", self.environment.shell_line(&arguments)),
            String::new(),
            "Then send this first message, and nothing else:".to_string(),
            format!("  {}", self.manifest.kickoff_message()),
            String::new(),
            format!(
                "Only these replies are allowed afterwards, and each is counted: {}",
                benchy::contract::PERMITTED_REPLIES
                    .map(|r| format!("\"{r}\""))
                    .join(" or ")
            ),
            format!(
                "When the harness reports done, exit it and run: benchy finish {}",
                self.id
            ),
        ];
        if !self.environment.has_login() {
            lines.push(login_warning(self.manifest.auth));
        }
        lines.join("\n")
    }
}

/// Searches every file under the run directory for the benchy checkout's path.
fn check_no_leak(run_dir: &Path, benchy_root: &Path) -> Result<(), RunError> {
    let needles = [
        benchy_root.display().to_string(),
        layout::canonical_or_given(benchy_root)
            .display()
            .to_string(),
    ];
    for file in files_under(run_dir)? {
        let Ok(bytes) = fs::read(&file) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        if needles.iter().any(|n| text.contains(n.as_str())) {
            return Err(RunError::Leak {
                benchy_root: needles[0].clone(),
                path: file.display().to_string(),
            });
        }
    }
    Ok(())
}

fn files_under(dir: &Path) -> Result<Vec<PathBuf>, RunError> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        let entries = fs::read_dir(&next).map_err(|source| RunError::Io {
            action: format!("read directory {}", next.display()),
            source,
        })?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    Ok(files)
}

/// A relocated configuration directory has no login, so the run needs its secret in the environment.
pub fn login_warning(auth: benchy::contract::AuthMode) -> String {
    match auth {
        benchy::contract::AuthMode::ApiKey => {
            "warning: the manifest uses an API key but ANTHROPIC_API_KEY is not set, so the run will not be logged in"
                .to_string()
        }
        benchy::contract::AuthMode::Subscription => format!(
            "warning: {} is not set, so the run will not be logged in; create one with `claude setup-token`",
            super::environment::OAUTH_TOKEN
        ),
    }
}
