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
    Status {
        #[arg(default_value = ".")]
        path: PathBuf,
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

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Commands::Status { path }) => show_status(&path),
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
    println!("  opsdeck list");
    println!("  opsdeck status");
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

    config.projects.sort_by(|first, second| {
        first
            .name
            .to_lowercase()
            .cmp(&second.name.to_lowercase())
    });

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

fn show_status(path: &Path) -> Result<(), String> {
    let repository_path = resolve_repository(path)?;

    let project_name = repository_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Proyecto desconocido");

    let branch_output = run_git(&repository_path, &["branch", "--show-current"])?;

    let branch = if branch_output.trim().is_empty() {
        let commit = run_git(&repository_path, &["rev-parse", "--short", "HEAD"])
            .unwrap_or_else(|_| "sin commits".to_string());

        format!("detached ({})", commit.trim())
    } else {
        branch_output.trim().to_string()
    };

    let status = run_git(&repository_path, &["status", "--short"])?;

    let changed_files = status
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();

    let last_commit = run_git(&repository_path, &["log", "-1", "--pretty=%s"])
        .map(|output| output.trim().to_string())
        .unwrap_or_else(|_| "Sin commits".to_string());

    let remote = run_git(&repository_path, &["remote", "get-url", "origin"])
        .map(|output| output.trim().to_string())
        .unwrap_or_else(|_| "Sin repositorio remoto".to_string());

    println!();
    println!("OPSDECK PROJECT STATUS");
    println!("────────────────────────────────────────");
    println!("Proyecto:             {project_name}");
    println!("Ruta:                 {}", repository_path.display());
    println!("Rama:                 {branch}");
    println!("Archivos modificados: {changed_files}");
    println!("Último commit:        {last_commit}");
    println!("Remoto:               {remote}");
    println!("────────────────────────────────────────");

    if changed_files == 0 {
        println!("Estado: repositorio limpio");
    } else {
        println!("Estado: hay cambios pendientes");
        println!();
        println!("{status}");
    }

    println!();

    Ok(())
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