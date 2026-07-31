use clap::{Parser, Subcommand};
use opsdeck::anomaly::detect_anomalies;
use opsdeck::checklist::{DeployChecklist, evaluate_deploy_checklist};
use opsdeck::gate::{DeployGate, evaluate_deploy_gate, export_gate_manifest};
use opsdeck::health::{HealthCheck, check_optional_url};
use opsdeck::history::{feedback_for_project, recent_reviews, record_review};
use opsdeck::intelligence::{Diagnosis, analyze_project_with_health};
use opsdeck::learning::apply_feedback;
use opsdeck::report::export_deploy_report;
use opsdeck::{
    ProjectStatus, add_project, config_path, load_config, open_in_file_manager, open_in_vscode,
    project_status, resolve_project_target,
};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "opsdeck",
    version,
    about = "Centro de control local para proyectos de desarrollo"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(alias = "st")]
    Status {
        #[arg(default_value = ".", value_name = "PROYECTO_O_RUTA")]
        target: String,
    },

    Diagnose {
        #[arg(default_value = ".", value_name = "PROYECTO_O_RUTA")]
        target: String,
    },

    Health {
        #[arg(default_value = ".", value_name = "PROYECTO_O_RUTA")]
        target: String,
    },

    Checklist {
        #[arg(default_value = ".", value_name = "PROYECTO_O_RUTA")]
        target: String,
    },

    Report {
        #[arg(default_value = ".", value_name = "PROYECTO_O_RUTA")]
        target: String,

        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    Gate {
        #[arg(default_value = ".", value_name = "PROYECTO_O_RUTA")]
        target: String,

        #[arg(long)]
        strict: bool,

        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    Add {
        name: String,

        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(long)]
        health_url: Option<String>,
    },

    List,

    Open {
        target: String,

        #[arg(long)]
        folder: bool,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,

        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Status { target }) => {
            let status = project_status(&target)?;

            print_status(&status);
            Ok(())
        }

        Some(Commands::Diagnose { target }) => {
            let status = project_status(&target)?;

            let health = check_optional_url(status.health_url.as_deref());

            let feedback = feedback_for_project(&status.name)?;

            let base_diagnosis = analyze_project_with_health(&status, &health);

            let diagnosis = apply_feedback(&status, base_diagnosis, &feedback);

            print_diagnosis(&diagnosis);
            Ok(())
        }

        Some(Commands::Health { target }) => {
            let project = resolve_project_target(&target)?;

            let health = check_optional_url(project.health_url.as_deref());

            print_health(&project.name, &health);
            Ok(())
        }

        Some(Commands::Checklist { target }) => {
            let status = project_status(&target)?;

            let health = check_optional_url(status.health_url.as_deref());

            let feedback = feedback_for_project(&status.name)?;

            let base_diagnosis = analyze_project_with_health(&status, &health);

            let diagnosis = apply_feedback(&status, base_diagnosis, &feedback);

            record_review(&status.name, &status, &health, &diagnosis)?;

            let history = recent_reviews(&status.name, 30)?;

            let anomaly_report = detect_anomalies(&history);

            let checklist =
                evaluate_deploy_checklist(&status, &health, &diagnosis, &anomaly_report);

            print_checklist(&status.name, &checklist);

            Ok(())
        }

        Some(Commands::Report { target, output }) => {
            let status = project_status(&target)?;

            let health = check_optional_url(status.health_url.as_deref());

            let feedback = feedback_for_project(&status.name)?;

            let base_diagnosis = analyze_project_with_health(&status, &health);

            let diagnosis = apply_feedback(&status, base_diagnosis, &feedback);

            record_review(&status.name, &status, &health, &diagnosis)?;

            let history = recent_reviews(&status.name, 30)?;

            let anomaly_report = detect_anomalies(&history);

            let checklist =
                evaluate_deploy_checklist(&status, &health, &diagnosis, &anomaly_report);

            let path = export_deploy_report(
                &status.name,
                &status,
                &health,
                &diagnosis,
                &history,
                &anomaly_report,
                output.as_deref(),
            )?;

            println!();
            println!("Informe de deploy generado");
            println!("────────────────────────────────────────");
            println!("Proyecto: {}", status.name);
            println!("Archivo:  {}", path.display());

            println!(
                "Decisión: {}",
                if checklist.ready {
                    if checklist.warnings > 0 {
                        "Permitido con advertencias"
                    } else {
                        "Aprobado"
                    }
                } else {
                    "Bloqueado"
                }
            );

            println!(
                "Aprobados: {} · Advertencias: {} · Bloqueados: {}",
                checklist.passed, checklist.warnings, checklist.failed
            );

            println!("────────────────────────────────────────");
            println!();

            Ok(())
        }

        Some(Commands::Gate {
            target,
            strict,
            output,
        }) => {
            let status = project_status(&target)?;

            let health = check_optional_url(status.health_url.as_deref());

            let feedback = feedback_for_project(&status.name)?;

            let base_diagnosis = analyze_project_with_health(&status, &health);

            let diagnosis = apply_feedback(&status, base_diagnosis, &feedback);

            record_review(&status.name, &status, &health, &diagnosis)?;

            let history = recent_reviews(&status.name, 30)?;

            let anomaly_report = detect_anomalies(&history);

            let checklist =
                evaluate_deploy_checklist(&status, &health, &diagnosis, &anomaly_report);

            let gate = evaluate_deploy_gate(
                &status.name,
                &status,
                &health,
                &diagnosis,
                &anomaly_report,
                &checklist,
                strict,
            );

            let path = export_gate_manifest(&gate, output.as_deref())?;

            print_gate(&gate, &path);

            if gate.ready {
                Ok(())
            } else {
                Err("OpsDeck Gate bloqueó el deploy".to_string())
            }
        }

        Some(Commands::Add {
            name,
            path,
            health_url,
        }) => {
            let project = add_project(name, path, health_url)?;

            println!();
            println!("Proyecto registrado");
            println!("────────────────────────────────────────");
            println!("Nombre: {}", project.name);
            println!("Ruta:   {}", project.path.display());

            match project.health_url {
                Some(url) => {
                    println!("Health: {url}");
                }

                None => {
                    println!("Health: sin endpoint");
                }
            }

            println!("────────────────────────────────────────");
            println!();

            Ok(())
        }

        Some(Commands::List) => print_projects(),

        Some(Commands::Open { target, folder }) => {
            let project = resolve_project_target(&target)?;

            if folder {
                open_in_file_manager(&project.path)?;

                println!("Carpeta abierta: {}", project.path.display());
            } else {
                open_in_vscode(&project.path)?;

                println!("Proyecto abierto en VS Code: {}", project.name);
            }

            Ok(())
        }

        None => {
            print_home();
            Ok(())
        }
    }
}

fn print_home() {
    println!();
    println!("OpsDeck");
    println!("Centro de control local para proyectos de desarrollo");
    println!();
    println!("Comandos disponibles:");
    println!("  opsdeck add <nombre> <ruta>");
    println!("  opsdeck add <nombre> <ruta> --health-url <url>");
    println!("  opsdeck list");
    println!("  opsdeck status <nombre>");
    println!("  opsdeck status <ruta>");
    println!("  opsdeck diagnose <nombre>");
    println!("  opsdeck diagnose <ruta>");
    println!("  opsdeck health <nombre>");
    println!("  opsdeck health <ruta>");
    println!("  opsdeck checklist <nombre>");
    println!("  opsdeck checklist <ruta>");
    println!("  opsdeck report <nombre>");
    println!("  opsdeck report <nombre> --output <archivo.md>");
    println!("  opsdeck gate <nombre>");
    println!("  opsdeck gate <nombre> --strict");
    println!("  opsdeck gate <nombre> --output <archivo.json>");
    println!("  opsdeck open <nombre>");
    println!("  opsdeck open <nombre> --folder");
    println!();
    println!("Aplicación gráfica:");
    println!("  cargo run --bin opsdeck-desktop");
    println!();
}

fn print_projects() -> Result<(), String> {
    let config = load_config()?;
    let path = config_path()?;

    println!();
    println!("OPSDECK PROJECTS");
    println!("────────────────────────────────────────");

    if config.projects.is_empty() {
        println!("No hay proyectos registrados");
        println!();
        println!("Registra uno con:");
        println!("opsdeck add \"Nombre\" /ruta/del/proyecto");
    } else {
        for (index, project) in config.projects.iter().enumerate() {
            println!("{}. {}", index + 1, project.name);

            println!("   Ruta: {}", project.path.display());

            match &project.health_url {
                Some(url) => {
                    println!("   Health: {url}");
                }

                None => {
                    println!("   Health: sin endpoint");
                }
            }

            if index + 1 < config.projects.len() {
                println!();
            }
        }
    }

    println!("────────────────────────────────────────");
    println!("Configuración: {}", path.display());
    println!();

    Ok(())
}

fn print_status(status: &ProjectStatus) {
    println!();
    println!("OPSDECK PROJECT STATUS");
    println!("──────────────────────────────────────────────────");
    println!("Proyecto:              {}", status.name);

    println!("Ruta:                  {}", status.path.display());

    println!(
        "Registrado:            {}",
        if status.registered { "sí" } else { "no" }
    );

    println!("Rama:                  {}", status.branch);

    println!("Archivos con cambios:  {}", status.changes.total);

    println!("Preparados:            {}", status.changes.staged);

    println!("Sin preparar:          {}", status.changes.unstaged);

    println!("Archivos nuevos:       {}", status.changes.untracked);

    println!("Último commit:         {}", status.last_commit);

    println!("Remoto:                {}", status.remote);

    match &status.sync.upstream {
        Some(upstream) => {
            println!("Seguimiento:           {upstream}");

            println!("Commits por subir:     {}", status.sync.ahead);

            println!("Commits por descargar: {}", status.sync.behind);
        }

        None => {
            println!("Seguimiento:           sin upstream");

            println!("Commits por subir:     no disponible");

            println!("Commits por descargar: no disponible");
        }
    }

    match &status.health_url {
        Some(url) => {
            println!("Health:                {url}");
        }

        None => {
            println!("Health:                sin endpoint");
        }
    }

    println!("──────────────────────────────────────────────────");

    println!("Estado: {}", status.state_label());

    if !status.raw_status.trim().is_empty() {
        println!();
        println!("CAMBIOS");

        println!("──────────────────────────────────────────────────");

        println!("{}", status.raw_status);
    }

    println!();
}

fn print_health(project_name: &str, health: &HealthCheck) {
    println!();
    println!("OPSDECK HEALTH CHECK");

    println!("──────────────────────────────────────────────────");

    println!("Proyecto:       {project_name}");
    println!("Estado:         {}", health.state);

    match &health.url {
        Some(url) => {
            println!("URL:            {url}");
        }

        None => {
            println!("URL:            sin configurar");
        }
    }

    match health.status_code {
        Some(code) => {
            println!("Código HTTP:    {code}");
        }

        None => {
            println!("Código HTTP:    no disponible");
        }
    }

    match health.latency_ms {
        Some(latency) => {
            println!("Latencia:       {latency} ms");
        }

        None => {
            println!("Latencia:       no disponible");
        }
    }

    match &health.content_type {
        Some(content_type) => {
            println!("Content-Type:   {content_type}");
        }

        None => {
            println!("Content-Type:   no disponible");
        }
    }

    match health.json_valid {
        Some(true) => {
            println!("JSON válido:    sí");
        }

        Some(false) => {
            println!("JSON válido:    no");
        }

        None => {
            println!("JSON válido:    no aplica");
        }
    }

    println!("──────────────────────────────────────────────────");

    if let Some(error) = &health.error {
        println!("Error: {error}");
    }

    if let Some(preview) = &health.body_preview {
        println!();
        println!("RESPUESTA");

        println!("──────────────────────────────────────────────────");

        println!("{preview}");
    }

    println!();
}

fn print_diagnosis(diagnosis: &Diagnosis) {
    println!();
    println!("OPSDECK INTELLIGENCE");

    println!("──────────────────────────────────────────────────");

    println!("Puntuación: {}/100", diagnosis.score);

    println!("Nivel:      {}", diagnosis.risk);
    println!();
    println!("{}", diagnosis.summary);

    println!("──────────────────────────────────────────────────");

    if diagnosis.findings.is_empty() {
        println!("No se encontraron problemas.");
    } else {
        for (index, finding) in diagnosis.findings.iter().enumerate() {
            println!();

            println!(
                "{}. {} [{}]",
                index + 1,
                finding.title,
                finding.severity.label()
            );

            println!("   Código: {}", finding.code);

            println!("   Análisis: {}", finding.explanation);

            println!("   Acción: {}", finding.action);

            println!("   Penalización adaptada: -{}", finding.penalty);
        }
    }

    println!();
}

fn print_checklist(project_name: &str, checklist: &DeployChecklist) {
    println!();
    println!("OPSDECK PRE-DEPLOY CHECKLIST");

    println!("──────────────────────────────────────────────────");

    println!("Proyecto: {project_name}");

    println!(
        "Decisión: {}",
        if checklist.ready {
            if checklist.warnings > 0 {
                "Deploy permitido con advertencias"
            } else {
                "Deploy aprobado"
            }
        } else {
            "Deploy bloqueado"
        }
    );

    println!(
        "Aprobados: {} · Advertencias: {} · Bloqueados: {}",
        checklist.passed, checklist.warnings, checklist.failed
    );

    println!();
    println!("{}", checklist.summary);

    println!("──────────────────────────────────────────────────");

    for (index, item) in checklist.items.iter().enumerate() {
        println!();

        println!("{}. [{}] {}", index + 1, item.state, item.title);

        println!("   Código: {}", item.code);
        println!("   Detalle: {}", item.detail);
    }

    println!();
}

fn print_gate(gate: &DeployGate, path: &Path) {
    println!();
    println!("OPSDECK DEPLOY GATE");

    println!("──────────────────────────────────────────────────");

    println!("Decisión: {}", gate.decision);

    println!("Listo:    {}", if gate.ready { "sí" } else { "no" });

    println!("Archivo:  {}", path.display());
    println!();
    println!("{}", gate.summary);

    if !gate.blockers.is_empty() {
        println!();
        println!("BLOQUEOS");

        for blocker in &gate.blockers {
            println!("  × {blocker}");
        }
    }

    if !gate.warnings.is_empty() {
        println!();
        println!("ADVERTENCIAS");

        for warning in &gate.warnings {
            println!("  ! {warning}");
        }
    }

    println!();
}
