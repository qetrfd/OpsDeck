use crate::history::ReviewRecord;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnomalySeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl fmt::Display for AnomalySeverity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Info => "Información",
            Self::Low => "Baja",
            Self::Medium => "Media",
            Self::High => "Alta",
            Self::Critical => "Crítica",
        };

        write!(formatter, "{value}")
    }
}

#[derive(Debug, Clone)]
pub struct Anomaly {
    pub code: String,
    pub title: String,
    pub explanation: String,
    pub action: String,
    pub severity: AnomalySeverity,
}

#[derive(Debug, Clone)]
pub struct AnomalyReport {
    pub anomalies: Vec<Anomaly>,
    pub deploy_ready: bool,
    pub summary: String,
}

pub fn detect_anomalies(reviews: &[ReviewRecord]) -> AnomalyReport {
    if reviews.is_empty() {
        return AnomalyReport {
            anomalies: Vec::new(),
            deploy_ready: true,
            summary: "Todavía no hay suficientes revisiones para detectar anomalías.".to_string(),
        };
    }

    let mut ordered = reviews.to_vec();
    ordered.sort_by_key(|review| review.checked_at);

    let Some(current) = ordered.last() else {
        return AnomalyReport {
            anomalies: Vec::new(),
            deploy_ready: true,
            summary: "No hay información disponible.".to_string(),
        };
    };

    let previous = if ordered.len() > 1 {
        &ordered[..ordered.len() - 1]
    } else {
        &[]
    };

    let mut anomalies = Vec::new();

    detect_score_drop(current, previous, &mut anomalies);
    detect_latency_spike(current, previous, &mut anomalies);
    detect_consecutive_health_failures(&ordered, &mut anomalies);
    detect_change_spike(current, previous, &mut anomalies);
    detect_sync_risk(current, &mut anomalies);
    detect_sensitive_findings(current, &mut anomalies);

    let deploy_ready = !anomalies.iter().any(|anomaly| {
        matches!(
            anomaly.severity,
            AnomalySeverity::High | AnomalySeverity::Critical
        )
    });

    let summary = build_summary(&anomalies, deploy_ready);

    AnomalyReport {
        anomalies,
        deploy_ready,
        summary,
    }
}

fn detect_score_drop(
    current: &ReviewRecord,
    previous: &[ReviewRecord],
    anomalies: &mut Vec<Anomaly>,
) {
    let Some(previous_review) = previous.last() else {
        return;
    };

    let drop = previous_review.score.saturating_sub(current.score);

    if drop >= 25 {
        anomalies.push(Anomaly {
            code: "CRITICAL_SCORE_DROP".to_string(),
            title: "La puntuación cayó de forma crítica".to_string(),
            explanation: format!(
                "La puntuación pasó de {}/100 a {}/100, una caída de {} puntos.",
                previous_review.score, current.score, drop
            ),
            action: "Revisa las nuevas recomendaciones y evita desplegar hasta explicar la caída."
                .to_string(),
            severity: AnomalySeverity::Critical,
        });
    } else if drop >= 15 {
        anomalies.push(Anomaly {
            code: "SIGNIFICANT_SCORE_DROP".to_string(),
            title: "La puntuación cayó significativamente".to_string(),
            explanation: format!(
                "La puntuación pasó de {}/100 a {}/100, una caída de {} puntos.",
                previous_review.score, current.score, drop
            ),
            action: "Compara los cambios recientes con la revisión anterior antes de continuar."
                .to_string(),
            severity: AnomalySeverity::High,
        });
    } else if drop >= 8 {
        anomalies.push(Anomaly {
            code: "MODERATE_SCORE_DROP".to_string(),
            title: "La puntuación comenzó a disminuir".to_string(),
            explanation: format!(
                "La puntuación disminuyó {} puntos desde la revisión anterior.",
                drop
            ),
            action: "Revisa qué reglas nuevas comenzaron a activarse.".to_string(),
            severity: AnomalySeverity::Medium,
        });
    }
}

fn detect_latency_spike(
    current: &ReviewRecord,
    previous: &[ReviewRecord],
    anomalies: &mut Vec<Anomaly>,
) {
    let Some(current_latency) = current.latency_ms else {
        return;
    };

    let previous_latencies = previous
        .iter()
        .filter_map(|review| review.latency_ms)
        .collect::<Vec<_>>();

    if previous_latencies.len() < 3 {
        return;
    }

    let average = previous_latencies.iter().sum::<u64>() / previous_latencies.len() as u64;

    if average == 0 {
        return;
    }

    if current_latency >= average.saturating_mul(4) && current_latency >= 1_000 {
        anomalies.push(Anomaly {
            code: "CRITICAL_LATENCY_SPIKE".to_string(),
            title: "La latencia aumentó de forma crítica".to_string(),
            explanation: format!(
                "La latencia actual es de {} ms, frente a un promedio histórico de {} ms.",
                current_latency, average
            ),
            action:
                "Revisa carga del servidor, base de datos, red y procesos lentos antes de desplegar."
                    .to_string(),
            severity: AnomalySeverity::Critical,
        });
    } else if current_latency >= average.saturating_mul(2)
        && current_latency.saturating_sub(average) >= 200
    {
        anomalies.push(Anomaly {
            code: "LATENCY_SPIKE".to_string(),
            title: "La latencia aumentó de forma anormal".to_string(),
            explanation: format!(
                "La latencia actual es de {} ms, mientras que el promedio anterior era de {} ms.",
                current_latency, average
            ),
            action: "Comprueba el rendimiento del servicio y repite la revisión antes del deploy."
                .to_string(),
            severity: AnomalySeverity::High,
        });
    }
}

fn detect_consecutive_health_failures(reviews: &[ReviewRecord], anomalies: &mut Vec<Anomaly>) {
    let consecutive_failures = reviews
        .iter()
        .rev()
        .take_while(|review| {
            !matches!(review.health_state.as_str(), "Saludable" | "Sin configurar")
        })
        .count();

    if consecutive_failures >= 3 {
        anomalies.push(Anomaly {
            code: "REPEATED_HEALTH_FAILURES".to_string(),
            title: "El servicio acumula fallos consecutivos".to_string(),
            explanation: format!(
                "Las últimas {} revisiones reportaron un estado de health problemático.",
                consecutive_failures
            ),
            action: "Detén el deploy y revisa disponibilidad, logs, DNS, HTTPS y configuración."
                .to_string(),
            severity: AnomalySeverity::Critical,
        });
    } else if consecutive_failures == 2 {
        anomalies.push(Anomaly {
            code: "REPEATED_HEALTH_WARNING".to_string(),
            title: "El servicio falló dos veces consecutivas".to_string(),
            explanation: "El problema podría ser persistente y no solamente una falla temporal."
                .to_string(),
            action: "Ejecuta otra revisión y consulta los logs del servicio.".to_string(),
            severity: AnomalySeverity::High,
        });
    } else if consecutive_failures == 1 {
        anomalies.push(Anomaly {
            code: "RECENT_HEALTH_FAILURE".to_string(),
            title: "La última revisión detectó un problema de health".to_string(),
            explanation: "El servicio presentó un fallo reciente que todavía debe confirmarse."
                .to_string(),
            action: "Repite la revisión antes de tomar una decisión de deploy.".to_string(),
            severity: AnomalySeverity::Medium,
        });
    }
}

fn detect_change_spike(
    current: &ReviewRecord,
    previous: &[ReviewRecord],
    anomalies: &mut Vec<Anomaly>,
) {
    if previous.len() < 3 {
        return;
    }

    let average = previous
        .iter()
        .map(|review| review.changes_total as u64)
        .sum::<u64>()
        / previous.len() as u64;

    let current_changes = current.changes_total as u64;

    if current_changes >= 40 && current_changes >= average.saturating_mul(3) {
        anomalies.push(Anomaly {
            code: "CRITICAL_CHANGE_SPIKE".to_string(),
            title: "El volumen de cambios creció de forma crítica".to_string(),
            explanation: format!(
                "Hay {} archivos con cambios, frente a un promedio histórico de {}.",
                current_changes, average
            ),
            action:
                "Divide el trabajo en commits pequeños y revisa archivos generados o inesperados."
                    .to_string(),
            severity: AnomalySeverity::High,
        });
    } else if current_changes >= 15 && current_changes >= average.saturating_mul(2) {
        anomalies.push(Anomaly {
            code: "CHANGE_SPIKE".to_string(),
            title: "El volumen de cambios es inusual".to_string(),
            explanation: format!(
                "Hay {} cambios locales, mientras que el promedio anterior era de {}.",
                current_changes, average
            ),
            action: "Comprueba que todos los archivos pertenezcan al mismo objetivo de trabajo."
                .to_string(),
            severity: AnomalySeverity::Medium,
        });
    }
}

fn detect_sync_risk(current: &ReviewRecord, anomalies: &mut Vec<Anomaly>) {
    if current.commits_ahead > 0 && current.commits_behind > 0 {
        anomalies.push(Anomaly {
            code: "SYNC_DIVERGENCE".to_string(),
            title: "La rama local y la remota están divergentes".to_string(),
            explanation: format!(
                "Existen {} commits por subir y {} por descargar.",
                current.commits_ahead, current.commits_behind
            ),
            action: "Resuelve la divergencia y vuelve a ejecutar OpsDeck antes del deploy."
                .to_string(),
            severity: AnomalySeverity::Critical,
        });
    } else if current.commits_behind > 0 {
        anomalies.push(Anomaly {
            code: "REMOTE_CHANGES_PENDING".to_string(),
            title: "Existen cambios remotos pendientes".to_string(),
            explanation: format!(
                "La rama local está {} commits detrás del remoto.",
                current.commits_behind
            ),
            action: "Descarga y valida los cambios remotos antes de realizar un despliegue."
                .to_string(),
            severity: AnomalySeverity::High,
        });
    }
}

fn detect_sensitive_findings(current: &ReviewRecord, anomalies: &mut Vec<Anomaly>) {
    let sensitive = current.finding_codes.iter().any(|code| {
        matches!(
            code.as_str(),
            "POTENTIAL_SECRET_FILE" | "BRANCH_DIVERGED" | "REPOSITORY_WITHOUT_COMMITS"
        )
    });

    if sensitive {
        anomalies.push(Anomaly {
            code: "BLOCKING_DIAGNOSTIC_RULE".to_string(),
            title: "Existe una regla que bloquea un deploy seguro".to_string(),
            explanation:
                "La revisión actual contiene una condición crítica de seguridad o repositorio."
                    .to_string(),
            action: "Resuelve las reglas críticas mostradas por OpsDeck Intelligence.".to_string(),
            severity: AnomalySeverity::Critical,
        });
    }
}

fn build_summary(anomalies: &[Anomaly], deploy_ready: bool) -> String {
    if anomalies.is_empty() {
        return "No se detectaron anomalías respecto al historial reciente.".to_string();
    }

    let critical = anomalies
        .iter()
        .filter(|anomaly| anomaly.severity == AnomalySeverity::Critical)
        .count();

    let high = anomalies
        .iter()
        .filter(|anomaly| anomaly.severity == AnomalySeverity::High)
        .count();

    if !deploy_ready {
        format!(
            "Deploy no recomendado: se detectaron {} anomalías, {} críticas y {} altas.",
            anomalies.len(),
            critical,
            high
        )
    } else {
        format!(
            "Se detectaron {} anomalías menores, pero ninguna bloquea el deploy.",
            anomalies.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review(score: u8, latency: Option<u64>) -> ReviewRecord {
        ReviewRecord {
            project_name: "Demo".to_string(),
            checked_at: 1,
            score,
            risk: "Saludable".to_string(),
            health_state: "Saludable".to_string(),
            status_code: Some(200),
            latency_ms: latency,
            branch: "main".to_string(),
            changes_total: 0,
            changes_staged: 0,
            changes_unstaged: 0,
            changes_untracked: 0,
            commits_ahead: 0,
            commits_behind: 0,
            finding_codes: Vec::new(),
        }
    }

    #[test]
    fn detects_significant_score_drop() {
        let mut first = review(100, Some(100));
        first.checked_at = 1;

        let mut second = review(80, Some(100));
        second.checked_at = 2;

        let result = detect_anomalies(&[first, second]);

        assert!(
            result
                .anomalies
                .iter()
                .any(|anomaly| anomaly.code == "SIGNIFICANT_SCORE_DROP")
        );
    }

    #[test]
    fn detects_latency_spike() {
        let mut reviews = Vec::new();

        for index in 0..4 {
            let mut item = review(100, Some(100));
            item.checked_at = index;
            reviews.push(item);
        }

        let mut current = review(100, Some(700));
        current.checked_at = 5;
        reviews.push(current);

        let result = detect_anomalies(&reviews);

        assert!(
            result
                .anomalies
                .iter()
                .any(|anomaly| anomaly.code == "LATENCY_SPIKE")
        );
    }

    #[test]
    fn divergent_repository_blocks_deploy() {
        let mut current = review(70, None);
        current.commits_ahead = 2;
        current.commits_behind = 3;

        let result = detect_anomalies(&[current]);

        assert!(!result.deploy_ready);
    }

    #[test]
    fn healthy_history_allows_deploy() {
        let result = detect_anomalies(&[
            review(100, Some(100)),
            review(100, Some(110)),
            review(98, Some(105)),
        ]);

        assert!(result.deploy_ready);
    }
}
