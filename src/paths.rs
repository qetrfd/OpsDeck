use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const DATA_DIRECTORY_NAME: &str = ".opsdeck";

#[cfg(target_os = "windows")]
const WINDOWS_DATA_DIRECTORY_NAME: &str = "OpsDeck";

pub fn data_dir() -> Result<PathBuf, String> {
    let directory = resolve_data_dir()?;

    fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "No se pudo crear la carpeta de datos {}: {error}",
            directory.display()
        )
    })?;

    Ok(directory)
}

pub fn data_file(name: impl AsRef<Path>) -> Result<PathBuf, String> {
    validate_relative_path(name.as_ref())?;

    Ok(data_dir()?.join(name))
}

pub fn data_subdir(name: impl AsRef<Path>) -> Result<PathBuf, String> {
    validate_relative_path(name.as_ref())?;

    let directory = data_dir()?.join(name);

    fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "No se pudo crear la carpeta {}: {error}",
            directory.display()
        )
    })?;

    Ok(directory)
}

fn resolve_data_dir() -> Result<PathBuf, String> {
    if let Some(custom_directory) = env::var_os("OPSDECK_HOME") {
        let directory = PathBuf::from(custom_directory);

        if !directory.as_os_str().is_empty() {
            return Ok(directory);
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
            return Ok(PathBuf::from(local_app_data).join(WINDOWS_DATA_DIRECTORY_NAME));
        }

        if let Some(user_profile) = env::var_os("USERPROFILE") {
            return Ok(PathBuf::from(user_profile).join(DATA_DIRECTORY_NAME));
        }

        if let Some(home) = env::var_os("HOME") {
            return Ok(PathBuf::from(home).join(DATA_DIRECTORY_NAME));
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = env::var_os("HOME") {
            return Ok(PathBuf::from(home).join(DATA_DIRECTORY_NAME));
        }
    }

    Err(format!(
        "No se pudo localizar la carpeta de datos de OpsDeck en {}.",
        env::consts::OS
    ))
}

fn validate_relative_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("El nombre del archivo o carpeta no puede estar vacío.".to_string());
    }

    if path.is_absolute() {
        return Err(format!(
            "La ruta debe ser relativa a la carpeta de OpsDeck: {}",
            path.display()
        ));
    }

    if path.components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return Err(format!(
            "La ruta no puede salir de la carpeta de OpsDeck: {}",
            path.display()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_relative_file_path() {
        assert!(validate_relative_path(Path::new("history.json"),).is_ok());
    }

    #[test]
    fn accepts_relative_subdirectory() {
        assert!(validate_relative_path(Path::new("reports/report.md"),).is_ok());
    }

    #[test]
    fn rejects_empty_path() {
        assert!(validate_relative_path(Path::new(""),).is_err());
    }

    #[test]
    fn rejects_parent_directory() {
        assert!(validate_relative_path(Path::new("../secrets.json"),).is_err());
    }

    #[test]
    fn rejects_absolute_path() {
        #[cfg(not(target_os = "windows"))]
        assert!(validate_relative_path(Path::new("/tmp/opsdeck"),).is_err());
    }
}
