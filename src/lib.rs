pub mod health;
pub mod history;
pub mod history_ui;
pub mod intelligence;
pub mod learning;
pub mod monitor;

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    pub health_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub projects: Vec<Project>,
}

#[derive(Debug, Clone)]
pub struct ResolvedProject {
    pub name: String,
    pub path: PathBuf,
    pub registered: bool,
    pub health_url: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ChangeSummary {
    pub total: usize,
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
}

#[derive(Debug, Clone)]
pub struct SyncStatus {
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
}

#[derive(Debug, Clone)]
pub struct ProjectStatus {
    pub name: String,
    pub path: PathBuf,
    pub registered: bool,
    pub health_url: Option<String>,
    pub branch: String,
    pub changes: ChangeSummary,
    pub last_commit: String,
    pub remote: String,
    pub sync: SyncStatus,
    pub raw_status: String,
}

impl ProjectStatus {
    pub fn state_label(&self) -> &'static str {
        if self.sync.ahead > 0 && self.sync.behind > 0 {
            return "Ramas divergentes";
        }

        if self.sync.behind > 0 && self.changes.total > 0 {
            return "Cambios locales y commits remotos pendientes";
        }

        if self.sync.behind > 0 {
            return "Commits remotos pendientes";
        }

        if self.sync.ahead > 0 && self.changes.total > 0 {
            return "Cambios locales y commits pendientes de subir";
        }

        if self.sync.ahead > 0 {
            return "Commits pendientes de subir";
        }

        if self.changes.total > 0 {
            return "Cambios locales pendientes";
        }

        if self.sync.upstream.is_none() {
            return "Repositorio limpio sin upstream";
        }

        "Repositorio limpio y sincronizado"
    }
}

pub fn add_project(
    name: String,
    path: PathBuf,
    health_url: Option<String>,
) -> Result<Project, String> {
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
        .any(|project| paths_match(&project.path, &repository_path))
    {
        return Err(format!(
            "La ruta ya está registrada: {}",
            repository_path.display()
        ));
    }

    let health_url = health_url
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty());

    let project = Project {
        name,
        path: repository_path,
        health_url,
    };

    config.projects.push(project.clone());

    config
        .projects
        .sort_by_key(|project| project.name.to_lowercase());

    save_config(&config)?;

    Ok(project)
}

pub fn load_config() -> Result<Config, String> {
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

pub fn save_config(config: &Config) -> Result<(), String> {
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

pub fn config_path() -> Result<PathBuf, String> {
    if let Ok(custom_path) = env::var("OPSDECK_CONFIG") {
        let custom_path = custom_path.trim();

        if !custom_path.is_empty() {
            return Ok(PathBuf::from(custom_path));
        }
    }

    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .map_err(|_| "No se pudo encontrar la carpeta del usuario".to_string())?;

    Ok(PathBuf::from(home).join(".opsdeck").join("projects.json"))
}

pub fn resolve_project_target(target: &str) -> Result<ResolvedProject, String> {
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
            .find(|project| paths_match(&project.path, &repository_path))
        {
            return Ok(ResolvedProject {
                name: project.name.clone(),
                path: repository_path,
                registered: true,
                health_url: project.health_url.clone(),
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
            health_url: None,
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
            health_url: project.health_url.clone(),
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

pub fn project_status(target: &str) -> Result<ProjectStatus, String> {
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

    let raw_status = run_git(path, &["status", "--short"])?;
    let changes = summarize_changes(&raw_status);
    let sync = get_sync_status(path);

    let last_commit = run_git(path, &["log", "-1", "--pretty=%h | %s | %cr"])
        .map(|output| output.trim().to_string())
        .unwrap_or_else(|_| "Sin commits".to_string());

    let remote = run_git(path, &["remote", "get-url", "origin"])
        .map(|output| output.trim().to_string())
        .unwrap_or_else(|_| "Sin repositorio remoto".to_string());

    Ok(ProjectStatus {
        name: project.name,
        path: project.path,
        registered: project.registered,
        health_url: project.health_url,
        branch,
        changes,
        last_commit,
        remote,
        sync,
        raw_status,
    })
}

pub fn open_in_vscode(path: &Path) -> Result<(), String> {
    if Command::new("code").arg(path).spawn().is_ok() {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        return Command::new("open")
            .arg("-a")
            .arg("Visual Studio Code")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("No se pudo abrir Visual Studio Code: {error}"));
    }

    #[cfg(target_os = "windows")]
    {
        return Command::new("cmd")
            .arg("/C")
            .arg("code")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("No se pudo abrir Visual Studio Code: {error}"));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return Command::new("code")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("No se pudo abrir Visual Studio Code: {error}"));
    }

    #[allow(unreachable_code)]
    Err("No se pudo abrir Visual Studio Code".to_string())
}

pub fn open_in_file_manager(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return Command::new("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("No se pudo abrir la carpeta: {error}"));
    }

    #[cfg(target_os = "windows")]
    {
        return Command::new("explorer")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("No se pudo abrir la carpeta: {error}"));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("No se pudo abrir la carpeta: {error}"));
    }

    #[allow(unreachable_code)]
    Err("No se pudo abrir la carpeta".to_string())
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
        &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
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

fn paths_match(first: &Path, second: &Path) -> bool {
    match (first.canonicalize(), second.canonicalize()) {
        (Ok(first), Ok(second)) => first == second,
        _ => first == second,
    }
}
