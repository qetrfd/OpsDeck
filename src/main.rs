use clap::{Parser, Subcommand};
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
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Status { path }) => match show_status(&path) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("Error: {error}");
                ExitCode::FAILURE
            }
        },
        None => {
            show_home();
            ExitCode::SUCCESS
        }
    }
}

fn show_home() {
    println!();
    println!("OpsDeck");
    println!("Panel local para administrar proyectos");
    println!();
    println!("Comandos disponibles:");
    println!("  opsdeck status");
    println!("  opsdeck status /ruta/del/proyecto");
    println!();
}

fn show_status(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("La ruta no existe: {}", path.display()));
    }

    if !path.is_dir() {
        return Err(format!("La ruta no es una carpeta: {}", path.display()));
    }

    let inside_repository = run_git(path, &["rev-parse", "--is-inside-work-tree"])?;

    if inside_repository.trim() != "true" {
        return Err("La carpeta no es un repositorio Git".to_string());
    }

    let absolute_path = path
        .canonicalize()
        .map_err(|error| format!("No se pudo resolver la ruta: {error}"))?;

    let project_name = absolute_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Proyecto desconocido");

    let branch_output = run_git(path, &["branch", "--show-current"])?;

    let branch = if branch_output.trim().is_empty() {
        let commit = run_git(path, &["rev-parse", "--short", "HEAD"])
            .unwrap_or_else(|_| "sin commits".to_string());

        format!("detached ({})", commit.trim())
    } else {
        branch_output.trim().to_string()
    };

    let status = run_git(path, &["status", "--short"])?;
    let changed_files = status
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();

    let last_commit = run_git(path, &["log", "-1", "--pretty=%s"])
        .map(|output| output.trim().to_string())
        .unwrap_or_else(|_| "Sin commits".to_string());

    let remote = run_git(path, &["remote", "get-url", "origin"])
        .map(|output| output.trim().to_string())
        .unwrap_or_else(|_| "Sin repositorio remoto".to_string());

    println!();
    println!("OPSDECK PROJECT STATUS");
    println!("────────────────────────────────────────");
    println!("Proyecto:            {project_name}");
    println!("Ruta:                {}", absolute_path.display());
    println!("Rama:                {branch}");
    println!("Archivos modificados: {changed_files}");
    println!("Último commit:       {last_commit}");
    println!("Remoto:              {remote}");
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