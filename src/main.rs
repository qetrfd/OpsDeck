use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

#[derive(Parser)]
#[command(
    name = "opsdeck",
    version,
    about = "Panel local para administrar proyectos de desarrollo"
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
    Add {
        name: String,
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        health_url: Option<String>,
    },
    List,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Project {
    name: String,
    path: PathBuf,
    health_url: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Config {
    projects: Vec<Project>,
}

struct ResolvedProject {
    name: String,
    path: PathBuf,
    registered: bool,
}

#[derive(Default)]
struct ChangeSummary {
    total: usize,
    staged: usize,
    unstaged: usize,
    untracked: usize,
}

struct SyncStatus {
    upstream: Option<String>,
    ahead: usize,
    behind: usize,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Commands::Status { target }) => show_status(&target),
        Some(Commands::Add {
            name,
            path,
            health_url,
        }) => add_project(name, path, health_url),
        Some(Commands::List) => list_projects(),
        None => {
            show_home();
            Ok(())
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn show_home() {
    println!();
    println!("OpsDeck");
    println!("Panel local para administrar proyectos");
    println!();
    println!("Comandos disponibles:");
    println!("  opsdeck add <nombre> <ruta>");
    println!("  opsdeck add <nombre> <ruta> --health-url <url>");
    println!("  opsdeck list");
    println!("  opsdeck status");
    println!("  opsdeck status <nombre>");
    println!("  opsdeck status <ruta>");
    println!();
}

fn add_project(
    name: String,
    path: PathBuf,
    health_url: Option<String>,
) -> Result<(), String> {
    let name = name.trim().to_string();

    if name.is_empty() {
        return Err("El nombre del proyecto no puede estar vacío".to_string());
    }

    let repository_path = resolve_repository(&path)?;
    let mut config = load_config()?;

    if config
        .projects
        .iter()
        .any(|project| project.name.eq_ignore_ascii_case(&name))
    {
        return Err(format!("Ya existe un proyecto llamado {name}"));
    }

    if config
        .projects
        .iter()
        .any(|project| project.path == repository_path)
    {
        return Err(format!(
            "La ruta ya está registrada: {}",
            repository_path.display()
        ));
    }

    let health_url = health_url
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty());

    config.projects.push(Project {
        name: name.clone(),
        path: repository_path.clone(),
        health_url,
    });

    config
        .projects
        .sort_by_key(|project| project.name.to_lowercase());

    save_config(&config)?;

    println!();
    println!("Proyecto registrado");
    println!("────────────────────────────────────────");
    println!("Nombre: {name}");
    println!("Ruta:   {}", repository_path.display());
    println!("────────────────────────────────────────");
    println!();

    Ok(())
}

fn list_projects() -> Result<(), String> {
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
                Some(url) => println!("   Health: {url}"),
                None => println!("   Health: sin endpoint"),
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

fn show_status(target: &str) -> Result<(), String> {
    let project = resolve_project_target(target)?;
    let path = &project.path;

    let branch_output = run_git(path, &["branch", "--show-current"])?;

    let branch = if branch_output.trim().is_empty() {
        let commit = run_git(path, &["rev-parse", "--short", "HEAD"])
            .unwrap_or_else(|_| "sin commits".to_string());

        format!("detached ({})", commit.trim())
    } else {
        branch_output.trim().to_string()
    };

    let status = run_git(path, &["status", "--short"])?;
    let changes = summarize_changes(&status);
    let sync = get_sync_status(path);

    let last_commit = run_git(path, &["log", "-1", "--pretty=%h | %s | %cr"])
        .map(|output| output.trim().to_string())
        .unwrap_or_else(|_| "Sin commits".to_string());

    let remote = run_git(path, &["remote", "get-url", "origin"])
        .map(|output| output.trim().to_string())
        .unwrap_or_else(|_| "Sin repositorio remoto".to_string());

    println!();
    println!("OPSDECK PROJECT STATUS");
    println!("──────────────────────────────────────────────────");
    println!("Proyecto:              {}", project.name);
    println!("Ruta:                  {}", path.display());
    println!(
        "Registrado:            {}",
        if project.registered { "sí" } else { "no" }
    );
    println!("Rama:                  {branch}");
    println!("Archivos con cambios:  {}", changes.total);
    println!("Preparados:            {}", changes.staged);
    println!("Sin preparar:          {}", changes.unstaged);
    println!("Archivos nuevos:       {}", changes.untracked);
    println!("Último commit:         {last_commit}");
    println!("Remoto:                {remote}");

    match &sync.upstream {
        Some(upstream) => {
            println!("Seguimiento:           {upstream}");
            println!("Commits por subir:     {}", sync.ahead);
            println!("Commits por descargar: {}", sync.behind);
        }
        None => {
            println!("Seguimiento:           sin upstream");
            println!("Commits por subir:     no disponible");
            println!("Commits por descargar: no disponible");
        }
    }

    println!("──────────────────────────────────────────────────");

    print_repository_state(&changes, &sync);

    if changes.total > 0 {
        println!();
        println!("CAMBIOS");
        println!("──────────────────────────────────────────────────");
        println!("{status}");
    }

    println!();

    Ok(())
}

fn print_repository_state(changes: &ChangeSummary, sync: &SyncStatus) {
    if changes.total == 0 && sync.ahead == 0 && sync.behind == 0 {
        println!("Estado: repositorio limpio y sincronizado");
        return;
    }

    if sync.ahead > 0 && sync.behind > 0 {
        println!("Estado: ramas divergentes");
        return;
    }

    if sync.behind > 0 && changes.total > 0 {
        println!("Estado: cambios locales y commits remotos pendientes");
        return;
    }

    if sync.behind > 0 {
        println!("Estado: hay commits remotos pendientes");
        return;
    }

    if sync.ahead > 0 && changes.total > 0 {
        println!("Estado: cambios locales y commits pendientes de subir");
        return;
    }

    if sync.ahead > 0 {
        println!("Estado: hay commits pendientes de subir");
        return;
    }

    println!("Estado: hay cambios locales pendientes");
}

fn resolve_project_target(target: &str) -> Result<ResolvedProject, String> {
    let target = target.trim();

    if target.is_empty() {
        return Err("Debes indicar un proyecto o una ruta".to_string());
    }

    let candidate = PathBuf::from(target);

    if candidate.exists() {
        let repository_path = resolve_repository(&candidate)?;
        let config = load_config()?;

        if let Some(project) = config
            .projects
            .iter()
            .find(|project| project.path == repository_path)
        {
            return Ok(ResolvedProject {
                name: project.name.clone(),
                path: repository_path,
                registered: true,
            });
        }

        let name = repository_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("Proyecto desconocido")
            .to_string();

        return Ok(ResolvedProject {
            name,
            path: repository_path,
            registered: false,
        });
    }

    let config = load_config()?;

    if let Some(project) = config
        .projects
        .iter()
        .find(|project| project.name.eq_ignore_ascii_case(target))
    {
        let repository_path = resolve_repository(&project.path).map_err(|error| {
            format!(
                "El proyecto {} está registrado, pero su ruta no está disponible: {error}",
                project.name
            )
        })?;

        return Ok(ResolvedProject {
            name: project.name.clone(),
            path: repository_path,
            registered: true,
        });
    }

    let available = config
        .projects
        .iter()
        .map(|project| project.name.as_str())
        .collect::<Vec<_>>();

    if available.is_empty() {
        return Err(format!(
            "No se encontró el proyecto o la ruta: {target}. No hay proyectos registrados"
        ));
    }

    Err(format!(
        "No se encontró \"{target}\". Proyectos disponibles: {}",
        available.join(", ")
    ))
}

fn summarize_changes(status: &str) -> ChangeSummary {
    let mut summary = ChangeSummary::default();

    for line in status.lines().filter(|line| !line.trim().is_empty()) {
        summary.total += 1;

        let bytes = line.as_bytes();

        if bytes.len() < 2 {
            continue;
        }

        let index_status = bytes[0];
        let working_status = bytes[1];

        if index_status == b'?' && working_status == b'?' {
            summary.untracked += 1;
            continue;
        }

        if index_status != b' ' {
            summary.staged += 1;
        }

        if working_status != b' ' {
            summary.unstaged += 1;
        }
    }

    summary
}

fn get_sync_status(path: &Path) -> SyncStatus {
    let upstream = match run_git(
        path,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    ) {
        Ok(output) if !output.trim().is_empty() => output.trim().to_string(),
        _ => {
            return SyncStatus {
                upstream: None,
                ahead: 0,
                behind: 0,
            };
        }
    };

    let counts = run_git(
        path,
        &[
            "rev-list",
            "--left-right",
            "--count",
            "HEAD...@{upstream}",
        ],
    )
    .unwrap_or_default();

    let mut values = counts.split_whitespace();

    let ahead = values
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);

    let behind = values
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);

    SyncStatus {
        upstream: Some(upstream),
        ahead,
        behind,
    }
}

fn resolve_repository(path: &Path) -> Result<PathBuf, String> {
    if !path.exists() {
        return Err(format!("La ruta no existe: {}", path.display()));
    }

    if !path.is_dir() {
        return Err(format!("La ruta no es una carpeta: {}", path.display()));
    }

    let repository_root = run_git(path, &["rev-parse", "--show-toplevel"])
        .map_err(|_| format!("La carpeta no es un repositorio Git: {}", path.display()))?;

    let repository_path = PathBuf::from(repository_root.trim());

    repository_path
        .canonicalize()
        .map_err(|error| format!("No se pudo resolver la ruta: {error}"))
}

fn config_path() -> Result<PathBuf, String> {
    if let Ok(custom_path) = env::var("OPSDECK_CONFIG") {
        let custom_path = custom_path.trim();

        if !custom_path.is_empty() {
            return Ok(PathBuf::from(custom_path));
        }
    }

    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .map_err(|_| "No se pudo encontrar la carpeta del usuario".to_string())?;

    Ok(PathBuf::from(home)
        .join(".opsdeck")
        .join("projects.json"))
}

fn load_config() -> Result<Config, String> {
    let path = config_path()?;

    if !path.exists() {
        return Ok(Config::default());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("No se pudo leer {}: {error}", path.display()))?;

    if content.trim().is_empty() {
        return Ok(Config::default());
    }

    serde_json::from_str(&content)
        .map_err(|error| format!("La configuración JSON no es válida: {error}"))
}

fn save_config(config: &Config) -> Result<(), String> {
    let path = config_path()?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("No se pudo crear {}: {error}", parent.display()))?;
    }

    let content = serde_json::to_string_pretty(config)
        .map_err(|error| format!("No se pudo generar el JSON: {error}"))?;

    fs::write(&path, content)
        .map_err(|error| format!("No se pudo guardar {}: {error}", path.display()))
}

fn run_git(path: &Path, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(path)
        .output()
        .map_err(|error| format!("No se pudo ejecutar Git: {error}"))?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        let message = error.trim();

        if message.is_empty() {
            return Err(format!(
                "Git terminó con el código {}",
                output.status.code().unwrap_or(1)
            ));
        }

        return Err(message.to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}