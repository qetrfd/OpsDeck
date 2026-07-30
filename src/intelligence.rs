use crate::ProjectStatus;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Healthy,
    Low,
    Medium,
    High,
    Critical,
}

impl fmt::Display for RiskLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Healthy => "Saludable",
            Self::Low => "Riesgo bajo",
            Self::Medium => "Riesgo medio",
            Self::High => "Riesgo alto",
            Self::Critical => "Riesgo crítico",
        };

        write!(formatter, "{label}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl FindingSeverity {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Info => "Información",
            Self::Low => "Bajo",
            Self::Medium => "Medio",
            Self::High => "Alto",
            Self::Critical => "Crítico",
        }
    }

    fn risk_level(&self) -> RiskLevel {
        match self {
            Self::Info => RiskLevel::Healthy,
            Self::Low => RiskLevel::Low,
            Self::Medium => RiskLevel::Medium,
            Self::High => RiskLevel::High,
            Self::Critical => RiskLevel::Critical,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub code: String,
    pub title: String,
    pub explanation: String,
    pub action: String,
    pub severity: FindingSeverity,
    pub penalty: u8,
}

#[derive(Debug, Clone)]
pub struct Diagnosis {
    pub score: u8,
    pub risk: RiskLevel,
    pub summary: String,
    pub findings: Vec<Finding>,
}

pub fn analyze_project(status: &ProjectStatus) -> Diagnosis {
    let mut findings = Vec::new();

    analyze_repository_history(status, &mut findings);
    analyze_remote_configuration(status, &mut findings);
    analyze_synchronization(status, &mut findings);
    analyze_working_tree(status, &mut findings);
    analyze_sensitive_files(status, &mut findings);
    analyze_branch_risk(status, &mut findings);
    analyze_health_configuration(status, &mut findings);

    let total_penalty = findings
        .iter()
        .map(|finding| finding.penalty as u16)
        .sum::<u16>()
        .min(100);

    let score = 100_u8.saturating_sub(total_penalty as u8);

    let score_risk = risk_from_score(score);

    let finding_risk = findings
        .iter()
        .map(|finding| finding.severity.risk_level())
        .max()
        .unwrap_or(RiskLevel::Healthy);

    let risk = score_risk.max(finding_risk);
    let summary = build_summary(status, score, risk, &findings);

    Diagnosis {
        score,
        risk,
        summary,
        findings,
    }
}

fn analyze_repository_history(status: &ProjectStatus, findings: &mut Vec<Finding>) {
    if status.last_commit.eq_ignore_ascii_case("Sin commits") {
        findings.push(Finding {
            code: "REPOSITORY_WITHOUT_COMMITS".to_string(),
            title: "El repositorio todavía no tiene commits".to_string(),
            explanation: "No existe un punto de recuperación en el historial de Git.".to_string(),
            action: "Crea un primer commit antes de continuar haciendo cambios importantes."
                .to_string(),
            severity: FindingSeverity::High,
            penalty: 20,
        });
    }
}

fn analyze_remote_configuration(status: &ProjectStatus, findings: &mut Vec<Finding>) {
    if status.remote.eq_ignore_ascii_case("Sin repositorio remoto") {
        findings.push(Finding {
            code: "REMOTE_MISSING".to_string(),
            title: "El repositorio no tiene un remoto configurado".to_string(),
            explanation: "Los commits solamente existen en el equipo local.".to_string(),
            action: "Configura un remoto y sube la rama principal para contar con respaldo."
                .to_string(),
            severity: FindingSeverity::Medium,
            penalty: 10,
        });
    }

    if status.sync.upstream.is_none()
        && !status.remote.eq_ignore_ascii_case("Sin repositorio remoto")
    {
        findings.push(Finding {
            code: "UPSTREAM_MISSING".to_string(),
            title: "La rama actual no tiene upstream".to_string(),
            explanation:
                "OpsDeck no puede determinar con precisión si hay commits por subir o descargar."
                    .to_string(),
            action: "Vincula la rama con el remoto usando git push -u origin nombre-de-rama."
                .to_string(),
            severity: FindingSeverity::Low,
            penalty: 6,
        });
    }
}

fn analyze_synchronization(status: &ProjectStatus, findings: &mut Vec<Finding>) {
    if status.sync.ahead > 0 && status.sync.behind > 0 {
        findings.push(Finding {
            code: "BRANCH_DIVERGED".to_string(),
            title: "La rama local y la remota están divergentes".to_string(),
            explanation: format!(
                "Existen {} commits locales por subir y {} commits remotos por descargar.",
                status.sync.ahead, status.sync.behind
            ),
            action: "Revisa el historial y resuelve la divergencia antes de hacer push o deploy."
                .to_string(),
            severity: FindingSeverity::Critical,
            penalty: 35,
        });

        return;
    }

    if status.sync.behind > 0 {
        findings.push(Finding {
            code: "REMOTE_COMMITS_PENDING".to_string(),
            title: "Hay commits remotos pendientes".to_string(),
            explanation: format!(
                "La rama local está {} commits detrás del remoto.",
                status.sync.behind
            ),
            action: "Descarga y revisa los cambios remotos antes de continuar trabajando."
                .to_string(),
            severity: FindingSeverity::High,
            penalty: 20,
        });
    }

    if status.sync.ahead >= 5 {
        findings.push(Finding {
            code: "MANY_UNPUSHED_COMMITS".to_string(),
            title: "Hay varios commits sin respaldo remoto".to_string(),
            explanation: format!(
                "Existen {} commits locales que todavía no han sido enviados.",
                status.sync.ahead
            ),
            action: "Revisa los commits y súbelos al remoto cuando estén listos.".to_string(),
            severity: FindingSeverity::Medium,
            penalty: 12,
        });
    } else if status.sync.ahead > 0 {
        findings.push(Finding {
            code: "UNPUSHED_COMMITS".to_string(),
            title: "Hay commits pendientes de subir".to_string(),
            explanation: format!(
                "Existen {} commits locales sin respaldo remoto.",
                status.sync.ahead
            ),
            action: "Haz push después de verificar que el proyecto compile correctamente."
                .to_string(),
            severity: FindingSeverity::Low,
            penalty: 4,
        });
    }
}

fn analyze_working_tree(status: &ProjectStatus, findings: &mut Vec<Finding>) {
    let total = status.changes.total;

    if total >= 50 {
        findings.push(Finding {
            code: "VERY_LARGE_WORKING_TREE".to_string(),
            title: "Hay demasiados archivos modificados".to_string(),
            explanation: format!(
                "El repositorio contiene {total} archivos con cambios pendientes."
            ),
            action: "Divide los cambios en commits pequeños y revisables antes de continuar."
                .to_string(),
            severity: FindingSeverity::High,
            penalty: 22,
        });
    } else if total >= 20 {
        findings.push(Finding {
            code: "LARGE_WORKING_TREE".to_string(),
            title: "El conjunto de cambios es grande".to_string(),
            explanation: format!(
                "El repositorio contiene {total} archivos con cambios pendientes."
            ),
            action: "Separa los cambios por función para evitar un commit difícil de revisar."
                .to_string(),
            severity: FindingSeverity::Medium,
            penalty: 13,
        });
    } else if total >= 5 {
        findings.push(Finding {
            code: "MULTIPLE_LOCAL_CHANGES".to_string(),
            title: "Hay varios cambios locales pendientes".to_string(),
            explanation: format!("El repositorio contiene {total} archivos modificados o nuevos."),
            action: "Revisa cada archivo y prepara commits con objetivos claros.".to_string(),
            severity: FindingSeverity::Low,
            penalty: 5,
        });
    }

    if status.changes.untracked >= 10 {
        findings.push(Finding {
            code: "MANY_UNTRACKED_FILES".to_string(),
            title: "Hay muchos archivos nuevos sin seguimiento".to_string(),
            explanation: format!(
                "Git detectó {} archivos nuevos que todavía no están siendo rastreados.",
                status.changes.untracked
            ),
            action: "Comprueba si deben agregarse al repositorio o incluirse en .gitignore."
                .to_string(),
            severity: FindingSeverity::Medium,
            penalty: 10,
        });
    } else if status.changes.untracked > 0 {
        findings.push(Finding {
            code: "UNTRACKED_FILES".to_string(),
            title: "Hay archivos nuevos sin seguimiento".to_string(),
            explanation: format!("Git detectó {} archivos nuevos.", status.changes.untracked),
            action:
                "Revisa los archivos antes de agregarlos para evitar subir información privada."
                    .to_string(),
            severity: FindingSeverity::Low,
            penalty: 4,
        });
    }

    if status.changes.staged > 0 && status.changes.unstaged > 0 {
        findings.push(Finding {
            code: "MIXED_STAGING_STATE".to_string(),
            title: "Hay cambios preparados y sin preparar al mismo tiempo".to_string(),
            explanation: "El próximo commit solamente incluirá una parte de los cambios actuales."
                .to_string(),
            action: "Revisa git diff y git diff --staged antes de crear el commit.".to_string(),
            severity: FindingSeverity::Low,
            penalty: 3,
        });
    }
}

fn analyze_sensitive_files(status: &ProjectStatus, findings: &mut Vec<Finding>) {
    let raw_status = status.raw_status.to_lowercase();

    let critical_patterns = [
        ".env",
        ".pem",
        ".p12",
        ".pfx",
        "service-account",
        "service_account",
        "credentials.json",
        "firebase-adminsdk",
        "id_rsa",
        "id_ed25519",
    ];

    let detected = critical_patterns
        .iter()
        .filter(|pattern| raw_status.contains(**pattern))
        .copied()
        .collect::<Vec<_>>();

    if !detected.is_empty() {
        findings.push(Finding {
            code: "POTENTIAL_SECRET_FILE".to_string(),
            title: "Posibles archivos sensibles detectados".to_string(),
            explanation: format!(
                "Los cambios contienen nombres asociados con secretos: {}.",
                detected.join(", ")
            ),
            action: "No hagas commit. Revisa los archivos, elimina secretos del staging y actualiza .gitignore."
                .to_string(),
            severity: FindingSeverity::Critical,
            penalty: 40,
        });
    }
}

fn analyze_branch_risk(status: &ProjectStatus, findings: &mut Vec<Finding>) {
    let protected_branch = matches!(status.branch.as_str(), "main" | "master" | "production");

    if protected_branch && status.changes.total >= 10 {
        findings.push(Finding {
            code: "LARGE_CHANGES_ON_PROTECTED_BRANCH".to_string(),
            title: "Hay un cambio grande directamente en una rama principal".to_string(),
            explanation: format!(
                "Se detectaron {} archivos con cambios en la rama {}.",
                status.changes.total, status.branch
            ),
            action: "Considera crear una rama de trabajo antes de continuar.".to_string(),
            severity: FindingSeverity::Medium,
            penalty: 10,
        });
    } else if protected_branch && status.changes.total > 0 {
        findings.push(Finding {
            code: "CHANGES_ON_PROTECTED_BRANCH".to_string(),
            title: "Estás trabajando directamente en la rama principal".to_string(),
            explanation: format!(
                "La rama {} contiene cambios locales pendientes.",
                status.branch
            ),
            action: "Valora mover los cambios a una rama separada antes de hacer commit."
                .to_string(),
            severity: FindingSeverity::Low,
            penalty: 3,
        });
    }
}

fn analyze_health_configuration(status: &ProjectStatus, findings: &mut Vec<Finding>) {
    if status.health_url.is_none() {
        findings.push(Finding {
            code: "HEALTH_ENDPOINT_MISSING".to_string(),
            title: "No hay endpoint de health configurado".to_string(),
            explanation:
                "OpsDeck todavía no puede comprobar automáticamente si el servicio está disponible."
                    .to_string(),
            action: "Agrega una URL de health si el proyecto expone un servicio web.".to_string(),
            severity: FindingSeverity::Info,
            penalty: 0,
        });
    }
}

fn risk_from_score(score: u8) -> RiskLevel {
    match score {
        91..=100 => RiskLevel::Healthy,
        76..=90 => RiskLevel::Low,
        56..=75 => RiskLevel::Medium,
        31..=55 => RiskLevel::High,
        _ => RiskLevel::Critical,
    }
}

fn build_summary(
    status: &ProjectStatus,
    score: u8,
    risk: RiskLevel,
    findings: &[Finding],
) -> String {
    if findings.is_empty() {
        return format!(
            "{} no presenta riesgos detectados y obtuvo una puntuación de {score}/100.",
            status.name
        );
    }

    let important = findings
        .iter()
        .filter(|finding| {
            matches!(
                finding.severity,
                FindingSeverity::High | FindingSeverity::Critical
            )
        })
        .count();

    match risk {
        RiskLevel::Healthy => format!(
            "{} se encuentra en buen estado con una puntuación de {score}/100.",
            status.name
        ),
        RiskLevel::Low => format!(
            "{} tiene detalles menores por revisar. Puntuación: {score}/100.",
            status.name
        ),
        RiskLevel::Medium => format!(
            "{} requiere atención antes del siguiente commit o deploy. Puntuación: {score}/100.",
            status.name
        ),
        RiskLevel::High => format!(
            "{} presenta {important} riesgos importantes. Evita desplegar hasta revisarlos. Puntuación: {score}/100.",
            status.name
        ),
        RiskLevel::Critical => format!(
            "{} presenta una condición crítica. Detén commits y despliegues hasta resolverla. Puntuación: {score}/100.",
            status.name
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChangeSummary, ProjectStatus, SyncStatus};
    use std::path::PathBuf;

    fn healthy_status() -> ProjectStatus {
        ProjectStatus {
            name: "Demo".to_string(),
            path: PathBuf::from("/tmp/demo"),
            registered: true,
            health_url: Some("https://example.com/health".to_string()),
            branch: "feature/demo".to_string(),
            changes: ChangeSummary::default(),
            last_commit: "abc123 | initial commit".to_string(),
            remote: "https://github.com/example/demo.git".to_string(),
            sync: SyncStatus {
                upstream: Some("origin/feature/demo".to_string()),
                ahead: 0,
                behind: 0,
            },
            raw_status: String::new(),
        }
    }

    #[test]
    fn healthy_project_receives_full_score() {
        let diagnosis = analyze_project(&healthy_status());

        assert_eq!(diagnosis.score, 100);
        assert_eq!(diagnosis.risk, RiskLevel::Healthy);
    }

    #[test]
    fn sensitive_file_creates_critical_diagnosis() {
        let mut status = healthy_status();
        status.changes.total = 1;
        status.changes.untracked = 1;
        status.raw_status = "?? .env".to_string();

        let diagnosis = analyze_project(&status);

        assert_eq!(diagnosis.risk, RiskLevel::Critical);
        assert!(
            diagnosis
                .findings
                .iter()
                .any(|finding| finding.code == "POTENTIAL_SECRET_FILE")
        );
    }

    #[test]
    fn divergent_branch_creates_critical_diagnosis() {
        let mut status = healthy_status();
        status.sync.ahead = 2;
        status.sync.behind = 3;

        let diagnosis = analyze_project(&status);

        assert_eq!(diagnosis.risk, RiskLevel::Critical);
    }
}
