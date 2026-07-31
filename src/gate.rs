use crate::ProjectStatus;
use crate::anomaly::AnomalyReport;
use crate::checklist::{CheckState, DeployChecklist};
use crate::health::HealthCheck;
use crate::intelligence::Diagnosis;
use serde::{Deserialize, Serialize};
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateDecision {
    Approved,
    ApprovedWithWarnings,
    Blocked,
}

impl fmt::Display for GateDecision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Approved => "Aprobado",
            Self::ApprovedWithWarnings => "Aprobado con advertencias",
            Self::Blocked => "Bloqueado",
        };

        write!(formatter, "{label}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateManifest {
    pub project_name: String,
    pub generated_at: u64,
    pub decision: String,
    pub ready: bool,
    pub strict_warnings: bool,
    pub score: u8,
    pub risk: String,
    pub branch: String,
    pub last_commit: String,
    pub health_state: String,
    pub status_code: Option<u16>,
    pub latency_ms: Option<u64>,
    pub checklist_passed: usize,
    pub checklist_warnings: usize,
    pub checklist_failed: usize,
    pub anomaly_count: usize,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DeployGate {
    pub decision: GateDecision,
    pub ready: bool,
    pub summary: String,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub manifest: GateManifest,
}

impl DeployGate {
    pub fn unavailable(project_name: impl Into<String>, reason: impl Into<String>) -> Self {
        let project_name = project_name.into();
        let reason = reason.into();

        let manifest = GateManifest {
            project_name,
            generated_at: unix_timestamp(),
            decision: GateDecision::Blocked.to_string(),
            ready: false,
            strict_warnings: false,
            score: 0,
            risk: "No disponible".to_string(),
            branch: "No disponible".to_string(),
            last_commit: "No disponible".to_string(),
            health_state: "No disponible".to_string(),
            status_code: None,
            latency_ms: None,
            checklist_passed: 0,
            checklist_warnings: 0,
            checklist_failed: 1,
            anomaly_count: 0,
            blockers: vec![reason.clone()],
            warnings: Vec::new(),
        };

        Self {
            decision: GateDecision::Blocked,
            ready: false,
            summary: format!(
                "Deploy bloqueado porque OpsDeck no pudo completar la evaluación: {reason}"
            ),
            blockers: vec![reason],
            warnings: Vec::new(),
            manifest,
        }
    }
}

pub fn evaluate_deploy_gate(
    project_name: &str,
    status: &ProjectStatus,
    health: &HealthCheck,
    diagnosis: &Diagnosis,
    anomaly_report: &AnomalyReport,
    checklist: &DeployChecklist,
    strict_warnings: bool,
) -> DeployGate {
    let blockers = checklist
        .items
        .iter()
        .filter(|item| item.state == CheckState::Failed)
        .map(|item| format!("{}: {}", item.code, item.title))
        .collect::<Vec<_>>();

    let warnings = checklist
        .items
        .iter()
        .filter(|item| item.state == CheckState::Warning)
        .map(|item| format!("{}: {}", item.code, item.title))
        .collect::<Vec<_>>();

    let decision = if !blockers.is_empty() {
        GateDecision::Blocked
    } else if strict_warnings && !warnings.is_empty() {
        GateDecision::Blocked
    } else if !warnings.is_empty() {
        GateDecision::ApprovedWithWarnings
    } else {
        GateDecision::Approved
    };

    let ready = decision != GateDecision::Blocked;

    let summary = match decision {
        GateDecision::Approved => format!(
            "Deploy aprobado. Los {} requisitos fueron superados.",
            checklist.passed
        ),

        GateDecision::ApprovedWithWarnings => format!(
            "Deploy aprobado con {} advertencia(s). No se detectaron bloqueos.",
            warnings.len()
        ),

        GateDecision::Blocked if strict_warnings && blockers.is_empty() => {
            format!(
                "Deploy bloqueado por política estricta: existen {} advertencia(s).",
                warnings.len()
            )
        }

        GateDecision::Blocked => format!(
            "Deploy bloqueado: existen {} requisito(s) fallidos y {} advertencia(s).",
            blockers.len(),
            warnings.len()
        ),
    };

    let manifest = GateManifest {
        project_name: project_name.to_string(),
        generated_at: unix_timestamp(),
        decision: decision.to_string(),
        ready,
        strict_warnings,
        score: diagnosis.score,
        risk: diagnosis.risk.to_string(),
        branch: status.branch.clone(),
        last_commit: status.last_commit.clone(),
        health_state: health.state.to_string(),
        status_code: health.status_code,
        latency_ms: health
            .latency_ms
            .map(|value| u64::try_from(value).unwrap_or(u64::MAX)),
        checklist_passed: checklist.passed,
        checklist_warnings: checklist.warnings,
        checklist_failed: checklist.failed,
        anomaly_count: anomaly_report.anomalies.len(),
        blockers: blockers.clone(),
        warnings: warnings.clone(),
    };

    DeployGate {
        decision,
        ready,
        summary,
        blockers,
        warnings,
        manifest,
    }
}

pub fn export_gate_manifest(gate: &DeployGate, output: Option<&Path>) -> Result<PathBuf, String> {
    let path = match output {
        Some(path) => path.to_path_buf(),
        None => default_gate_path(&gate.manifest.project_name)?,
    };

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("No se pudo crear {}: {error}", parent.display()))?;
    }

    let content = serde_json::to_string_pretty(&gate.manifest)
        .map_err(|error| format!("No se pudo generar el manifiesto JSON: {error}"))?;

    fs::write(&path, content)
        .map_err(|error| format!("No se pudo guardar {}: {error}", path.display()))?;

    Ok(path)
}

pub fn suggested_gate_filename(project_name: &str) -> String {
    format!("{}-deploy-gate.json", slugify(project_name))
}

pub fn default_gate_path(project_name: &str) -> Result<PathBuf, String> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "No se pudo localizar la carpeta del usuario".to_string())?;

    let directory = home.join(".opsdeck").join("gates");

    fs::create_dir_all(&directory)
        .map_err(|error| format!("No se pudo crear {}: {error}", directory.display()))?;

    Ok(directory.join(format!(
        "{}-deploy-gate-{}.json",
        slugify(project_name),
        unix_timestamp()
    )))
}

fn slugify(value: &str) -> String {
    let mut result = String::new();
    let mut separator = false;

    for character in value.trim().to_lowercase().chars() {
        if character.is_alphanumeric() {
            result.push(character);
            separator = false;
        } else if !separator && !result.is_empty() {
            result.push('-');
            separator = true;
        }
    }

    while result.ends_with('-') {
        result.pop();
    }

    if result.is_empty() {
        "project".to_string()
    } else {
        result
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checklist::{ChecklistItem, DeployChecklist};

    fn checklist(passed: usize, warnings: usize, failed: usize) -> DeployChecklist {
        let mut items = Vec::new();

        for index in 0..passed {
            items.push(ChecklistItem {
                code: format!("PASS_{index}"),
                title: "Aprobado".to_string(),
                detail: "Correcto".to_string(),
                state: CheckState::Passed,
            });
        }

        for index in 0..warnings {
            items.push(ChecklistItem {
                code: format!("WARNING_{index}"),
                title: "Advertencia".to_string(),
                detail: "Revisar".to_string(),
                state: CheckState::Warning,
            });
        }

        for index in 0..failed {
            items.push(ChecklistItem {
                code: format!("FAILED_{index}"),
                title: "Bloqueado".to_string(),
                detail: "Corregir".to_string(),
                state: CheckState::Failed,
            });
        }

        DeployChecklist {
            items,
            passed,
            warnings,
            failed,
            ready: failed == 0,
            summary: String::new(),
        }
    }

    #[test]
    fn checklist_without_warnings_is_approved() {
        let checklist = checklist(9, 0, 0);

        let decision = if checklist.failed > 0 {
            GateDecision::Blocked
        } else if checklist.warnings > 0 {
            GateDecision::ApprovedWithWarnings
        } else {
            GateDecision::Approved
        };

        assert_eq!(decision, GateDecision::Approved);
    }

    #[test]
    fn warnings_are_allowed_in_normal_mode() {
        let checklist = checklist(8, 1, 0);

        let decision = if checklist.failed > 0 {
            GateDecision::Blocked
        } else if checklist.warnings > 0 {
            GateDecision::ApprovedWithWarnings
        } else {
            GateDecision::Approved
        };

        assert_eq!(decision, GateDecision::ApprovedWithWarnings);
    }

    #[test]
    fn failed_item_blocks_gate() {
        let checklist = checklist(7, 1, 1);

        assert!(!checklist.ready);
        assert_eq!(checklist.failed, 1);
    }

    #[test]
    fn creates_safe_manifest_filename() {
        assert_eq!(
            suggested_gate_filename("Kuali Web"),
            "kuali-web-deploy-gate.json"
        );
    }
}
