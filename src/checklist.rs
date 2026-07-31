use crate::ProjectStatus;
use crate::anomaly::AnomalyReport;
use crate::health::{HealthCheck, HealthState};
use crate::intelligence::{Diagnosis, FindingSeverity};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Passed,
    Warning,
    Failed,
}

impl fmt::Display for CheckState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Passed => "Aprobado",
            Self::Warning => "Advertencia",
            Self::Failed => "Bloqueado",
        };

        write!(formatter, "{label}")
    }
}

#[derive(Debug, Clone)]
pub struct ChecklistItem {
    pub code: String,
    pub title: String,
    pub detail: String,
    pub state: CheckState,
}

#[derive(Debug, Clone)]
pub struct DeployChecklist {
    pub items: Vec<ChecklistItem>,
    pub passed: usize,
    pub warnings: usize,
    pub failed: usize,
    pub ready: bool,
    pub summary: String,
}

impl DeployChecklist {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        let item = ChecklistItem {
            code: "CHECKLIST_UNAVAILABLE".to_string(),
            title: "No se pudo completar la verificación".to_string(),
            detail: reason.into(),
            state: CheckState::Failed,
        };

        Self {
            items: vec![item],
            passed: 0,
            warnings: 0,
            failed: 1,
            ready: false,
            summary: "Deploy bloqueado porque la verificación no pudo completarse.".to_string(),
        }
    }
}

pub fn evaluate_deploy_checklist(
    status: &ProjectStatus,
    health: &HealthCheck,
    diagnosis: &Diagnosis,
    anomaly_report: &AnomalyReport,
) -> DeployChecklist {
    let mut items = Vec::new();

    check_commit_history(status, &mut items);
    check_remote(status, &mut items);
    check_upstream(status, &mut items);
    check_synchronization(status, &mut items);
    check_working_tree(status, &mut items);
    check_sensitive_files(diagnosis, &mut items);
    check_health(health, &mut items);
    check_diagnosis(diagnosis, &mut items);
    check_anomalies(anomaly_report, &mut items);

    build_checklist(items)
}

fn check_commit_history(status: &ProjectStatus, items: &mut Vec<ChecklistItem>) {
    if status.last_commit.eq_ignore_ascii_case("Sin commits") {
        items.push(ChecklistItem {
            code: "COMMIT_HISTORY".to_string(),
            title: "El repositorio debe tener historial".to_string(),
            detail: "No existe ningún commit que pueda utilizarse como punto de recuperación."
                .to_string(),
            state: CheckState::Failed,
        });
    } else {
        items.push(ChecklistItem {
            code: "COMMIT_HISTORY".to_string(),
            title: "El repositorio tiene historial".to_string(),
            detail: format!("Último commit: {}", status.last_commit),
            state: CheckState::Passed,
        });
    }
}

fn check_remote(status: &ProjectStatus, items: &mut Vec<ChecklistItem>) {
    if status
        .remote
        .to_lowercase()
        .contains("sin repositorio remoto")
    {
        items.push(ChecklistItem {
            code: "REMOTE_CONFIGURED".to_string(),
            title: "Debe existir un repositorio remoto".to_string(),
            detail: "Los commits solamente están almacenados en el equipo local.".to_string(),
            state: CheckState::Failed,
        });
    } else {
        items.push(ChecklistItem {
            code: "REMOTE_CONFIGURED".to_string(),
            title: "Repositorio remoto configurado".to_string(),
            detail: status.remote.clone(),
            state: CheckState::Passed,
        });
    }
}

fn check_upstream(status: &ProjectStatus, items: &mut Vec<ChecklistItem>) {
    match &status.sync.upstream {
        Some(upstream) => {
            items.push(ChecklistItem {
                code: "UPSTREAM_CONFIGURED".to_string(),
                title: "La rama tiene upstream".to_string(),
                detail: format!("La rama sigue a {upstream}."),
                state: CheckState::Passed,
            });
        }

        None => {
            items.push(ChecklistItem {
                code: "UPSTREAM_CONFIGURED".to_string(),
                title: "La rama no tiene upstream".to_string(),
                detail: "OpsDeck no puede confirmar completamente la sincronización con el remoto."
                    .to_string(),
                state: CheckState::Warning,
            });
        }
    }
}

fn check_synchronization(status: &ProjectStatus, items: &mut Vec<ChecklistItem>) {
    if status.sync.ahead > 0 && status.sync.behind > 0 {
        items.push(ChecklistItem {
            code: "BRANCH_SYNCHRONIZED".to_string(),
            title: "La rama local y la remota están divergentes".to_string(),
            detail: format!(
                "{} commits por subir y {} por descargar.",
                status.sync.ahead, status.sync.behind
            ),
            state: CheckState::Failed,
        });

        return;
    }

    if status.sync.behind > 0 {
        items.push(ChecklistItem {
            code: "BRANCH_SYNCHRONIZED".to_string(),
            title: "Hay commits remotos pendientes".to_string(),
            detail: format!("La rama local está {} commits detrás.", status.sync.behind),
            state: CheckState::Failed,
        });

        return;
    }

    if status.sync.ahead > 0 {
        items.push(ChecklistItem {
            code: "BRANCH_SYNCHRONIZED".to_string(),
            title: "Hay commits locales pendientes de subir".to_string(),
            detail: format!(
                "{} commits todavía no tienen respaldo remoto.",
                status.sync.ahead
            ),
            state: CheckState::Warning,
        });

        return;
    }

    items.push(ChecklistItem {
        code: "BRANCH_SYNCHRONIZED".to_string(),
        title: "La rama está sincronizada".to_string(),
        detail: "No hay commits pendientes de subir o descargar.".to_string(),
        state: CheckState::Passed,
    });
}

fn check_working_tree(status: &ProjectStatus, items: &mut Vec<ChecklistItem>) {
    if status.changes.total == 0 {
        items.push(ChecklistItem {
            code: "WORKING_TREE_CLEAN".to_string(),
            title: "El árbol de trabajo está limpio".to_string(),
            detail: "No existen cambios locales pendientes.".to_string(),
            state: CheckState::Passed,
        });

        return;
    }

    items.push(ChecklistItem {
        code: "WORKING_TREE_CLEAN".to_string(),
        title: "Existen cambios locales sin guardar".to_string(),
        detail: format!(
            "{} archivos tienen cambios: {} preparados, {} sin preparar y {} nuevos.",
            status.changes.total,
            status.changes.staged,
            status.changes.unstaged,
            status.changes.untracked
        ),
        state: CheckState::Failed,
    });
}

fn check_sensitive_files(diagnosis: &Diagnosis, items: &mut Vec<ChecklistItem>) {
    let sensitive_file = diagnosis
        .findings
        .iter()
        .any(|finding| finding.code == "POTENTIAL_SECRET_FILE");

    if sensitive_file {
        items.push(ChecklistItem {
            code: "NO_SENSITIVE_FILES".to_string(),
            title: "Se detectaron posibles archivos sensibles".to_string(),
            detail: "El deploy está bloqueado hasta retirar secretos, credenciales o llaves."
                .to_string(),
            state: CheckState::Failed,
        });
    } else {
        items.push(ChecklistItem {
            code: "NO_SENSITIVE_FILES".to_string(),
            title: "No se detectaron archivos sensibles".to_string(),
            detail: "Las reglas locales no encontraron nombres asociados con secretos.".to_string(),
            state: CheckState::Passed,
        });
    }
}

fn check_health(health: &HealthCheck, items: &mut Vec<ChecklistItem>) {
    match health.state {
        HealthState::Healthy => {
            items.push(ChecklistItem {
                code: "HEALTH_AVAILABLE".to_string(),
                title: "El servicio está disponible".to_string(),
                detail: match health.latency_ms {
                    Some(latency) => {
                        format!("Respuesta saludable en {latency} ms.")
                    }

                    None => "El endpoint respondió correctamente.".to_string(),
                },
                state: CheckState::Passed,
            });
        }

        HealthState::NotConfigured => {
            items.push(ChecklistItem {
                code: "HEALTH_AVAILABLE".to_string(),
                title: "No hay health URL configurada".to_string(),
                detail: "La disponibilidad del servicio no puede comprobarse automáticamente."
                    .to_string(),
                state: CheckState::Warning,
            });
        }

        HealthState::Degraded => {
            items.push(ChecklistItem {
                code: "HEALTH_AVAILABLE".to_string(),
                title: "El servicio presenta degradación".to_string(),
                detail: "El endpoint respondió, pero su rendimiento o contenido requiere revisión."
                    .to_string(),
                state: CheckState::Warning,
            });
        }

        HealthState::Unhealthy
        | HealthState::Timeout
        | HealthState::Unreachable
        | HealthState::InvalidUrl => {
            items.push(ChecklistItem {
                code: "HEALTH_AVAILABLE".to_string(),
                title: "El health check no fue aprobado".to_string(),
                detail: health
                    .error
                    .clone()
                    .unwrap_or_else(|| format!("Estado detectado: {}.", health.state)),
                state: CheckState::Failed,
            });
        }
    }
}

fn check_diagnosis(diagnosis: &Diagnosis, items: &mut Vec<ChecklistItem>) {
    let has_critical = diagnosis
        .findings
        .iter()
        .any(|finding| matches!(finding.severity, FindingSeverity::Critical));

    let has_high = diagnosis
        .findings
        .iter()
        .any(|finding| matches!(finding.severity, FindingSeverity::High));

    if has_critical {
        items.push(ChecklistItem {
            code: "INTELLIGENCE_SCORE".to_string(),
            title: "El diagnóstico contiene riesgos críticos".to_string(),
            detail: format!("Puntuación actual: {}/100.", diagnosis.score),
            state: CheckState::Failed,
        });

        return;
    }

    if has_high || diagnosis.score < 75 {
        items.push(ChecklistItem {
            code: "INTELLIGENCE_SCORE".to_string(),
            title: "El diagnóstico requiere atención".to_string(),
            detail: format!(
                "La puntuación actual es {}/100 y existen riesgos importantes.",
                diagnosis.score
            ),
            state: CheckState::Failed,
        });

        return;
    }

    if diagnosis.score < 90 {
        items.push(ChecklistItem {
            code: "INTELLIGENCE_SCORE".to_string(),
            title: "El diagnóstico tiene observaciones".to_string(),
            detail: format!("La puntuación actual es {}/100.", diagnosis.score),
            state: CheckState::Warning,
        });

        return;
    }

    items.push(ChecklistItem {
        code: "INTELLIGENCE_SCORE".to_string(),
        title: "El diagnóstico fue aprobado".to_string(),
        detail: format!("Puntuación actual: {}/100.", diagnosis.score),
        state: CheckState::Passed,
    });
}

fn check_anomalies(report: &AnomalyReport, items: &mut Vec<ChecklistItem>) {
    if report.deploy_ready {
        let state = if report.anomalies.is_empty() {
            CheckState::Passed
        } else {
            CheckState::Warning
        };

        items.push(ChecklistItem {
            code: "HISTORICAL_ANOMALIES".to_string(),
            title: if report.anomalies.is_empty() {
                "No se detectaron anomalías históricas".to_string()
            } else {
                "Se detectaron anomalías menores".to_string()
            },
            detail: report.summary.clone(),
            state,
        });
    } else {
        items.push(ChecklistItem {
            code: "HISTORICAL_ANOMALIES".to_string(),
            title: "El historial contiene anomalías bloqueantes".to_string(),
            detail: report.summary.clone(),
            state: CheckState::Failed,
        });
    }
}

fn build_checklist(items: Vec<ChecklistItem>) -> DeployChecklist {
    let passed = items
        .iter()
        .filter(|item| item.state == CheckState::Passed)
        .count();

    let warnings = items
        .iter()
        .filter(|item| item.state == CheckState::Warning)
        .count();

    let failed = items
        .iter()
        .filter(|item| item.state == CheckState::Failed)
        .count();

    let ready = failed == 0;

    let summary = if failed > 0 {
        format!(
            "Deploy bloqueado: {failed} requisito(s) fallaron, {warnings} advertencia(s) y {passed} aprobados."
        )
    } else if warnings > 0 {
        format!(
            "Deploy permitido con {warnings} advertencia(s). {passed} requisito(s) fueron aprobados."
        )
    } else {
        format!("Deploy aprobado: los {passed} requisitos fueron superados.")
    };

    DeployChecklist {
        items,
        passed,
        warnings,
        failed,
        ready,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anomaly::AnomalyReport;
    use crate::health::{HealthCheck, HealthState};
    use crate::intelligence::{Diagnosis, Finding, FindingSeverity, RiskLevel};
    use crate::{ChangeSummary, ProjectStatus, SyncStatus};
    use std::path::PathBuf;

    fn status() -> ProjectStatus {
        ProjectStatus {
            name: "Demo".to_string(),
            path: PathBuf::from("/tmp/demo"),
            registered: true,
            health_url: Some("https://example.com/health".to_string()),
            branch: "main".to_string(),
            changes: ChangeSummary::default(),
            last_commit: "abc123 | initial commit".to_string(),
            remote: "https://github.com/example/demo.git".to_string(),
            sync: SyncStatus {
                upstream: Some("origin/main".to_string()),
                ahead: 0,
                behind: 0,
            },
            raw_status: String::new(),
        }
    }

    fn health() -> HealthCheck {
        HealthCheck {
            url: Some("https://example.com/health".to_string()),
            state: HealthState::Healthy,
            status_code: Some(200),
            latency_ms: Some(100),
            content_type: Some("application/json".to_string()),
            json_valid: Some(true),
            body_preview: None,
            error: None,
        }
    }

    fn diagnosis() -> Diagnosis {
        Diagnosis {
            score: 100,
            risk: RiskLevel::Healthy,
            summary: "Sin problemas".to_string(),
            findings: Vec::new(),
        }
    }

    fn anomalies() -> AnomalyReport {
        AnomalyReport {
            anomalies: Vec::new(),
            deploy_ready: true,
            summary: "No se detectaron anomalías.".to_string(),
        }
    }

    #[test]
    fn healthy_project_is_ready() {
        let result = evaluate_deploy_checklist(&status(), &health(), &diagnosis(), &anomalies());

        assert!(result.ready);
        assert_eq!(result.failed, 0);
    }

    #[test]
    fn local_changes_block_deploy() {
        let mut status = status();
        status.changes.total = 2;
        status.changes.unstaged = 2;

        let result = evaluate_deploy_checklist(&status, &health(), &diagnosis(), &anomalies());

        assert!(!result.ready);

        assert!(
            result.items.iter().any(|item| {
                item.code == "WORKING_TREE_CLEAN" && item.state == CheckState::Failed
            })
        );
    }

    #[test]
    fn sensitive_file_blocks_deploy() {
        let mut diagnosis = diagnosis();

        diagnosis.findings.push(Finding {
            code: "POTENTIAL_SECRET_FILE".to_string(),
            title: "Archivo sensible".to_string(),
            explanation: "Se encontró .env".to_string(),
            action: "Retirar archivo".to_string(),
            severity: FindingSeverity::Critical,
            penalty: 40,
        });

        let result = evaluate_deploy_checklist(&status(), &health(), &diagnosis, &anomalies());

        assert!(!result.ready);
    }
}
