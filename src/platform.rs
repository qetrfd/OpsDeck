use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::env;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::path::PathBuf;

pub fn platform_name() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macOS"
    }

    #[cfg(target_os = "windows")]
    {
        "Windows"
    }

    #[cfg(target_os = "linux")]
    {
        "Linux"
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        "Sistema no compatible"
    }
}

pub fn open_in_file_manager(path: &Path) -> Result<(), String> {
    validate_target(path)?;

    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        command.arg(path);

        launch_first_available("abrir la carpeta en Finder", path, vec![("open", command)])
    }

    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("explorer.exe");
        command.arg(path);

        launch_first_available(
            "abrir la carpeta en el Explorador de archivos",
            path,
            vec![("explorer.exe", command)],
        )
    }

    #[cfg(target_os = "linux")]
    {
        let mut xdg_open = Command::new("xdg-open");
        xdg_open.arg(path);

        let mut gio = Command::new("gio");
        gio.arg("open").arg(path);

        let mut kde_open = Command::new("kde-open5");
        kde_open.arg(path);

        launch_first_available(
            "abrir la carpeta en el administrador de archivos",
            path,
            vec![
                ("xdg-open", xdg_open),
                ("gio open", gio),
                ("kde-open5", kde_open),
            ],
        )
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(format!(
            "OpsDeck no puede abrir carpetas automáticamente en {}.",
            platform_name()
        ))
    }
}

pub fn open_in_vscode(path: &Path) -> Result<(), String> {
    validate_target(path)?;

    #[cfg(target_os = "macos")]
    {
        open_in_vscode_macos(path)
    }

    #[cfg(target_os = "windows")]
    {
        open_in_vscode_windows(path)
    }

    #[cfg(target_os = "linux")]
    {
        open_in_vscode_linux(path)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(format!(
            "OpsDeck no puede abrir Visual Studio Code automáticamente en {}.",
            platform_name()
        ))
    }
}

#[cfg(target_os = "macos")]
fn open_in_vscode_macos(path: &Path) -> Result<(), String> {
    let mut code = Command::new("code");
    code.arg(path);

    let mut application_cli =
        Command::new("/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code");
    application_cli.arg(path);

    let user_application_path = user_vscode_path();

    let mut user_application_cli = match user_application_path {
        Some(path) => Command::new(path),
        None => Command::new("code"),
    };

    user_application_cli.arg(path);

    let mut open_application = Command::new("open");
    open_application
        .arg("-a")
        .arg("Visual Studio Code")
        .arg(path);

    launch_first_available(
        "abrir el proyecto en Visual Studio Code",
        path,
        vec![
            ("code", code),
            ("/Applications/Visual Studio Code.app", application_cli),
            (
                "~/Applications/Visual Studio Code.app",
                user_application_cli,
            ),
            ("open -a Visual Studio Code", open_application),
        ],
    )
}

#[cfg(target_os = "linux")]
fn open_in_vscode_linux(path: &Path) -> Result<(), String> {
    let mut code = Command::new("code");
    code.arg(path);

    let mut code_insiders = Command::new("code-insiders");
    code_insiders.arg(path);

    let mut codium = Command::new("codium");
    codium.arg(path);

    let mut flatpak_code = Command::new("flatpak");
    flatpak_code
        .arg("run")
        .arg("com.visualstudio.code")
        .arg(path);

    let mut flatpak_codium = Command::new("flatpak");
    flatpak_codium
        .arg("run")
        .arg("com.vscodium.codium")
        .arg(path);

    launch_first_available(
        "abrir el proyecto en Visual Studio Code",
        path,
        vec![
            ("code", code),
            ("code-insiders", code_insiders),
            ("codium", codium),
            ("flatpak com.visualstudio.code", flatpak_code),
            ("flatpak com.vscodium.codium", flatpak_codium),
        ],
    )
}

#[cfg(target_os = "windows")]
fn open_in_vscode_windows(path: &Path) -> Result<(), String> {
    let mut candidates = Vec::<(&'static str, Command)>::new();

    let mut code_cmd = Command::new("code.cmd");
    code_cmd.arg(path);
    candidates.push(("code.cmd", code_cmd));

    let mut code_exe = Command::new("code.exe");
    code_exe.arg(path);
    candidates.push(("code.exe", code_exe));

    if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
        let executable = PathBuf::from(local_app_data)
            .join("Programs")
            .join("Microsoft VS Code")
            .join("Code.exe");

        let mut command = Command::new(executable);
        command.arg(path);

        candidates.push((
            "%LOCALAPPDATA%\\Programs\\Microsoft VS Code\\Code.exe",
            command,
        ));
    }

    if let Some(program_files) = env::var_os("ProgramFiles") {
        let executable = PathBuf::from(program_files)
            .join("Microsoft VS Code")
            .join("Code.exe");

        let mut command = Command::new(executable);
        command.arg(path);

        candidates.push(("%ProgramFiles%\\Microsoft VS Code\\Code.exe", command));
    }

    if let Some(program_files_x86) = env::var_os("ProgramFiles(x86)") {
        let executable = PathBuf::from(program_files_x86)
            .join("Microsoft VS Code")
            .join("Code.exe");

        let mut command = Command::new(executable);
        command.arg(path);

        candidates.push(("%ProgramFiles(x86)%\\Microsoft VS Code\\Code.exe", command));
    }

    launch_first_available("abrir el proyecto en Visual Studio Code", path, candidates)
}

#[cfg(target_os = "macos")]
fn user_vscode_path() -> Option<PathBuf> {
    env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join("Applications")
            .join("Visual Studio Code.app")
            .join("Contents")
            .join("Resources")
            .join("app")
            .join("bin")
            .join("code")
    })
}

fn validate_target(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("No se proporcionó una ruta para abrir.".to_string());
    }

    if !path.exists() {
        return Err(format!("La ruta no existe: {}", path.display()));
    }

    if !path.is_dir() {
        return Err(format!(
            "La ruta no corresponde a una carpeta: {}",
            path.display()
        ));
    }

    Ok(())
}

fn launch_first_available(
    action: &str,
    path: &Path,
    candidates: Vec<(&str, Command)>,
) -> Result<(), String> {
    let mut errors = Vec::<String>::new();

    for (label, mut command) in candidates {
        match spawn_detached(&mut command) {
            Ok(()) => return Ok(()),

            Err(error) => {
                errors.push(format!("{label}: {error}"));
            }
        }
    }

    let attempts = if errors.is_empty() {
        "No se encontraron comandos disponibles.".to_string()
    } else {
        errors.join(" | ")
    };

    Err(format!(
        "No se pudo {action} para {}. Intentos: {attempts}",
        path.display()
    ))
}

fn spawn_detached(command: &mut Command) -> io::Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        command.creation_flags(CREATE_NO_WINDOW);
    }

    command.spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn platform_name_is_available() {
        assert!(!platform_name().trim().is_empty());
    }

    #[test]
    fn missing_path_is_rejected() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        let path = env::temp_dir().join(format!("opsdeck-missing-platform-test-{unique}"));

        let result = open_in_file_manager(&path);

        assert!(result.is_err());
    }

    #[test]
    fn regular_file_is_rejected() {
        let path =
            env::temp_dir().join(format!("opsdeck-platform-file-test-{}", std::process::id()));

        std::fs::write(&path, "OpsDeck").unwrap();

        let result = open_in_file_manager(&path);

        let _ = std::fs::remove_file(&path);

        assert!(result.is_err());
    }
}
