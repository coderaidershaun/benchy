//! Closes a run: refuses while its processes live, snapshots the committed workspace, archives the transcripts,
//! writes the telemetry and runs the audit. Raw transcripts stay under `.runs/`; only derived numbers leave it.

use std::fs;
use std::path::{Path, PathBuf};

use benchy::contract::{HarnessManifest, Prices, Telemetry};

use super::RunError;
use super::layout::{self, OperatorDir, RunDir, RunId};
use super::processes;
use super::record::{FinishRecord, RunRecord, write_json};
use super::workspace::{self, copy_tree};
use crate::audit::{self, AuditReport, AuditScope};
use crate::collect::{self, manual::ManualEntry, timestamp, transcript};

#[derive(Debug, Clone)]
pub struct Finish {
    pub benchy_root: PathBuf,
    pub id: RunId,
    /// A manual entry file; when absent, `.runs/<run-id>/manual.toml` is used if it exists.
    pub manual: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Finished {
    pub record: RunRecord,
    pub telemetry: Telemetry,
    pub audit: Option<AuditReport>,
    /// Why the audit could not run, when it could not.
    pub audit_error: Option<String>,
}

impl Finish {
    /// # Errors
    ///
    /// Fails while processes of the run are alive, when git or a copy fails, or when the telemetry can be neither
    /// collected nor entered by hand. The snapshot, archive and audit are written before a telemetry error is returned.
    pub fn run(&self) -> Result<Finished, RunError> {
        let operator = OperatorDir::new(&self.benchy_root, &self.id);
        let mut record = RunRecord::load(&operator.record())?;
        let run = RunDir {
            dir: record.run_dir.clone(),
        };
        let alive = processes::alive_in(&run.dir)?;
        if !alive.is_empty() {
            return Err(RunError::StillAlive {
                pids: alive.iter().map(|p| p.pid).collect(),
            });
        }
        let snapshot = workspace::snapshot(&run.workspace(), &operator.workspace_tarball())?;
        let archive = operator.transcripts();
        if archive.exists() {
            fs::remove_dir_all(&archive).map_err(|source| RunError::Io {
                action: format!("remove the old archive {}", archive.display()),
                source,
            })?;
        }
        if run.projects().is_dir() {
            copy_tree(&run.projects(), &archive)?;
        } else {
            fs::create_dir_all(&archive).map_err(|source| RunError::Io {
                action: format!("create {}", archive.display()),
                source,
            })?;
        }
        let (audit, audit_error) = match self.audit(&archive, &run) {
            Ok(report) => {
                write_json(&operator.audit(), &report)?;
                (Some(report), None)
            }
            Err(error) => (None, Some(error_chain(&error))),
        };
        record.finish = Some(FinishRecord {
            ended: timestamp::format(timestamp::now()),
            head: snapshot.head,
            branch: snapshot.branch,
            uncommitted: snapshot.uncommitted,
            audit_findings: audit
                .as_ref()
                .map(|a| u32::try_from(a.findings.len()).unwrap_or(u32::MAX)),
        });
        record.save(&operator.record())?;
        let telemetry = self.telemetry(&archive, &operator)?;
        write_json(&operator.telemetry(), &telemetry)?;
        Ok(Finished {
            record,
            telemetry,
            audit,
            audit_error,
        })
    }

    fn audit(
        &self,
        archive: &Path,
        run: &RunDir,
    ) -> Result<AuditReport, crate::collect::CollectError> {
        let transcripts = transcript::read_projects(archive)?;
        let scope = AuditScope {
            run_dir: layout::canonical_or_given(&run.dir),
            home: layout::home().unwrap_or_default(),
            benchy_root: layout::canonical_or_given(&self.benchy_root),
        };
        Ok(audit::audit(&transcripts, &scope))
    }

    fn telemetry(&self, archive: &Path, operator: &OperatorDir) -> Result<Telemetry, RunError> {
        let prices = Prices::load(&self.benchy_root.join("validation/prices.toml"))?;
        let manifest = HarnessManifest::load(&operator.manifest())?;
        let manual_path = self.manual.clone().or_else(|| {
            operator
                .manual_entry()
                .is_file()
                .then(|| operator.manual_entry())
        });
        let collected = collect::collect(archive, &prices);
        let mut telemetry = match (collected, manual_path) {
            (Ok(telemetry), None) => telemetry,
            (Ok(telemetry), Some(path)) => ManualEntry::load(&path)?.apply(Some(telemetry))?,
            (Err(_), Some(path)) => ManualEntry::load(&path)?.apply(None)?,
            (Err(source), None) => {
                return Err(RunError::Collect {
                    manual: operator.manual_entry().display().to_string(),
                    source,
                });
            }
        };
        // A fold from 0.0, because the sum of no floats is -0.0 and would print as "-0.00".
        telemetry.external_spend_usd = manifest
            .external_spend
            .iter()
            .fold(0.0, |total, spend| total + spend.usd);
        Ok(telemetry)
    }
}

fn error_chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}
