use crate::ProjectStatus;
use crate::anomaly::AnomalyReport;
use crate::health::HealthCheck;
use crate::history::ReviewRecord;
use crate::intelligence::Diagnosis;
use std::env;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const HISTORY_ROWS: usize = 15;

pub fn generate_deploy_report(
    project_name: &str,
    status: &ProjectStatus,
    health: &HealthCheck,
    diagnosis: &Diagnosis,
    history: &[ReviewRecord],
    anomaly_report: &AnomalyReport,
) -> String {
    let generated_at = unix_timestamp();

    let decision = if anomaly_report.deploy_ready {
        "APROBADO"
    } else {
        "NO RECOMENDADO"
    };

    let decision_symbol = if anomaly_report.deploy_ready {
        "✅"
    } else {
        "⛔"
    };

    let mut report = String::new();

    writeln!(report, "# OpsDeck Deploy Report").unwrap();
    writeln!(report).unwrap();
    writeln!(report, "**Proyecto:** {project_name}").unwrap();
    writeln!(report, "**Generado:** {generated_at}").unwrap();
    writeln!(report, "**Decisión:** {decision_symbol} {decision}").unwrap();
    writeln!(report).unwrap();

    writeln!(report, "## Resumen ejecutivo").unwrap();
    writeln!(report).unwrap();
    writeln!(report, "{}", anomaly_report.summary).unwrap();
    writeln!(report).unwrap();
    writeln!(report, "- Puntuación: **{}/100**", diagnosis.score).unwrap();
    writeln!(report, "- Nivel de riesgo: **{}**", diagnosis.risk).unwrap();
    writeln!(report, "- Estado de health: **{}**", health.state).unwrap();
    writeln!(
        report,
        "- Anomalías detectadas: **{}**",
        anomaly_report.anomalies.len()
    )
    .unwrap();
    writeln!(report).unwrap();

    writeln!(report, "## Estado del repositorio").unwrap();
    writeln!(report).unwrap();
    writeln!(report, "| Campo | Valor |").unwrap();
    writeln!(report, "|---|---|").unwrap();
    writeln!(report, "| Ruta | `{}` |", status.path.display()).unwrap();
    writeln!(report, "| Rama | `{}` |", status.branch).unwrap();
    writeln!(
        report,
        "| Último commit | {} |",
        markdown_cell(&status.last_commit)
    )
    .unwrap();
    writeln!(report, "| Remoto | {} |", markdown_cell(&status.remote)).unwrap();
    writeln!(
        report,
        "| Upstream | {} |",
        status
            .sync
            .upstream
            .as_deref()
            .map(markdown_cell)
            .unwrap_or_else(|| "Sin upstream".to_string())
    )
    .unwrap();
    writeln!(report, "| Commits por subir | {} |", status.sync.ahead).unwrap();
    writeln!(report, "| Commits por descargar | {} |", status.sync.behind).unwrap();
    writeln!(report, "| Cambios totales | {} |", status.changes.total).unwrap();
    writeln!(report, "| Cambios preparados | {} |", status.changes.staged).unwrap();
    writeln!(
        report,
        "| Cambios sin preparar | {} |",
        status.changes.unstaged
    )
    .unwrap();
    writeln!(report, "| Archivos nuevos | {} |", status.changes.untracked).unwrap();
    writeln!(report).unwrap();

    writeln!(report, "## Health check").unwrap();
    writeln!(report).unwrap();
    writeln!(report, "| Campo | Valor |").unwrap();
    writeln!(report, "|---|---|").unwrap();
    writeln!(report, "| Estado | {} |", health.state).unwrap();
    writeln!(
        report,
        "| URL | {} |",
        health
            .url
            .as_deref()
            .map(markdown_cell)
            .unwrap_or_else(|| "Sin configurar".to_string())
    )
    .unwrap();
    writeln!(
        report,
        "| Código HTTP | {} |",
        health
            .status_code
            .map(|value| value.to_string())
            .unwrap_or_else(|| "No disponible".to_string())
    )
    .unwrap();
    writeln!(
        report,
        "| Latencia | {} |",
        health
            .latency_ms
            .map(|value| format!("{value} ms"))
            .unwrap_or_else(|| "No disponible".to_string())
    )
    .unwrap();
    writeln!(
        report,
        "| Content-Type | {} |",
        health
            .content_type
            .as_deref()
            .map(markdown_cell)
            .unwrap_or_else(|| "No disponible".to_string())
    )
    .unwrap();

    let json_valid = match health.json_valid {
        Some(true) => "Sí",
        Some(false) => "No",
        None => "No aplica",
    };

    writeln!(report, "| JSON válido | {json_valid} |").unwrap();

    if let Some(error) = &health.error {
        writeln!(report, "| Error | {} |", markdown_cell(error)).unwrap();
    }

    writeln!(report).unwrap();

    writeln!(report, "## OpsDeck Intelligence").unwrap();
    writeln!(report).unwrap();
    writeln!(report, "{}", diagnosis.summary).unwrap();
    writeln!(report).unwrap();

    if diagnosis.findings.is_empty() {
        writeln!(report, "No se encontraron recomendaciones activas.").unwrap();
    } else {
        for finding in &diagnosis.findings {
            writeln!(
                report,
                "### {} · {}",
                finding.severity.label(),
                finding.title
            )
            .unwrap();
            writeln!(report).unwrap();
            writeln!(report, "- **Regla:** `{}`", finding.code).unwrap();
            writeln!(report, "- **Penalización adaptada:** -{}", finding.penalty).unwrap();
            writeln!(report, "- **Análisis:** {}", finding.explanation).unwrap();
            writeln!(report, "- **Acción recomendada:** {}", finding.action).unwrap();
            writeln!(report).unwrap();
        }
    }

    writeln!(report, "## Análisis de anomalías").unwrap();
    writeln!(report).unwrap();

    if anomaly_report.anomalies.is_empty() {
        writeln!(
            report,
            "No se detectaron anomalías respecto al historial reciente."
        )
        .unwrap();
    } else {
        for anomaly in &anomaly_report.anomalies {
            writeln!(report, "### {} · {}", anomaly.severity, anomaly.title).unwrap();
            writeln!(report).unwrap();
            writeln!(report, "- **Código:** `{}`", anomaly.code).unwrap();
            writeln!(report, "- **Explicación:** {}", anomaly.explanation).unwrap();
            writeln!(report, "- **Acción recomendada:** {}", anomaly.action).unwrap();
            writeln!(report).unwrap();
        }
    }

    writeln!(report, "## Historial reciente").unwrap();
    writeln!(report).unwrap();

    if history.is_empty() {
        writeln!(report, "No hay revisiones históricas disponibles.").unwrap();
    } else {
        writeln!(
            report,
            "| Fecha | Puntuación | Riesgo | Health | HTTP | Latencia | Cambios |"
        )
        .unwrap();
        writeln!(report, "|---:|---:|---|---|---:|---:|---:|").unwrap();

        for review in history.iter().take(HISTORY_ROWS) {
            let status_code = review
                .status_code
                .map(|value| value.to_string())
                .unwrap_or_else(|| "—".to_string());

            let latency = review
                .latency_ms
                .map(|value| format!("{value} ms"))
                .unwrap_or_else(|| "—".to_string());

            writeln!(
                report,
                "| {} | {}/100 | {} | {} | {} | {} | {} |",
                review.checked_at,
                review.score,
                markdown_cell(&review.risk),
                markdown_cell(&review.health_state),
                status_code,
                latency,
                review.changes_total
            )
            .unwrap();
        }
    }

    writeln!(report).unwrap();
    writeln!(report, "## Cambios locales").unwrap();
    writeln!(report).unwrap();

    if status.raw_status.trim().is_empty() {
        writeln!(report, "No hay cambios locales pendientes.").unwrap();
    } else {
        writeln!(report, "```text").unwrap();
        writeln!(report, "{}", status.raw_status.trim()).unwrap();
        writeln!(report, "```").unwrap();
    }

    writeln!(report).unwrap();
    writeln!(report, "---").unwrap();
    writeln!(report, "Informe generado localmente por OpsDeck.").unwrap();

    report
}

pub fn export_deploy_report(
    project_name: &str,
    status: &ProjectStatus,
    health: &HealthCheck,
    diagnosis: &Diagnosis,
    history: &[ReviewRecord],
    anomaly_report: &AnomalyReport,
    output: Option<&Path>,
) -> Result<PathBuf, String> {
    let content = generate_deploy_report(
        project_name,
        status,
        health,
        diagnosis,
        history,
        anomaly_report,
    );

    let path = match output {
        Some(path) => path.to_path_buf(),
        None => default_report_path(project_name)?,
    };

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!("No se pudo crear la carpeta {}: {error}", parent.display())
        })?;
    }

    fs::write(&path, content)
        .map_err(|error| format!("No se pudo guardar el informe {}: {error}", path.display()))?;

    Ok(path)
}

pub fn suggested_report_filename(project_name: &str) -> String {
    format!("{}-deploy-report.md", slugify(project_name))
}

pub fn default_report_path(project_name: &str) -> Result<PathBuf, String> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "No se pudo localizar la carpeta del usuario".to_string())?;

    let directory = home.join(".opsdeck").join("reports");

    fs::create_dir_all(&directory)
        .map_err(|error| format!("No se pudo crear {}: {error}", directory.display()))?;

    Ok(directory.join(format!(
        "{}-deploy-report-{}.md",
        slugify(project_name),
        unix_timestamp()
    )))
}

fn slugify(value: &str) -> String {
    let mut result = String::new();
    let mut previous_separator = false;

    for character in value.trim().to_lowercase().chars() {
        if character.is_alphanumeric() {
            result.push(character);
            previous_separator = false;
        } else if !previous_separator && !result.is_empty() {
            result.push('-');
            previous_separator = true;
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

fn markdown_cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace('\n', " ")
        .replace('\r', " ")
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

    #[test]
    fn creates_safe_report_filename() {
        assert_eq!(
            suggested_report_filename("Kuali Web"),
            "kuali-web-deploy-report.md"
        );
    }

    #[test]
    fn empty_project_name_has_fallback() {
        assert_eq!(suggested_report_filename("   "), "project-deploy-report.md");
    }

    #[test]
    fn markdown_table_cells_are_escaped() {
        assert_eq!(markdown_cell("main | production"), "main \\| production");
    }
}
