//! Пути и правила хранения локальных файлов `mtrm`.

use std::fs;
use std::path::{Path, PathBuf};

use directories::BaseDirs;
use thiserror::Error;

const DATA_DIR_NAME: &str = ".mtrm";
const STATE_FILE_NAME: &str = "state.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtrmPaths {
    pub home_dir: PathBuf,
    pub data_dir: PathBuf,
    pub state_file: PathBuf,
}

impl MtrmPaths {
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn state_file(&self) -> &Path {
        &self.state_file
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("home directory is unavailable")]
    HomeDirUnavailable,
    #[error("failed to create directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub fn resolve_paths() -> Result<MtrmPaths, ConfigError> {
    let home_dir = BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .ok_or(ConfigError::HomeDirUnavailable)?;
    resolve_paths_from_home(home_dir)
}

pub fn ensure_data_dir() -> Result<MtrmPaths, ConfigError> {
    let paths = resolve_paths()?;
    ensure_data_dir_at(&paths)?;
    Ok(paths)
}

fn resolve_paths_from_home(home_dir: PathBuf) -> Result<MtrmPaths, ConfigError> {
    if home_dir.as_os_str().is_empty() {
        return Err(ConfigError::HomeDirUnavailable);
    }

    let data_dir = home_dir.join(DATA_DIR_NAME);
    let state_file = data_dir.join(STATE_FILE_NAME);

    Ok(MtrmPaths {
        home_dir,
        data_dir,
        state_file,
    })
}

fn ensure_data_dir_at(paths: &MtrmPaths) -> Result<(), ConfigError> {
    fs::create_dir_all(&paths.data_dir).map_err(|source| ConfigError::CreateDir {
        path: paths.data_dir.clone(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    use tempfile::tempdir;

    #[test]
    fn resolve_paths_uses_dot_mtrm_directory() {
        let home = PathBuf::from("/tmp/example-home");
        let paths = resolve_paths_from_home(home.clone()).unwrap();

        assert_eq!(paths.home_dir, home);
        assert_eq!(paths.data_dir, home.join(".mtrm"));
    }

    #[test]
    fn resolve_paths_uses_state_toml_file() {
        let home = PathBuf::from("/tmp/example-home");
        let paths = resolve_paths_from_home(home.clone()).unwrap();

        assert_eq!(paths.state_file, home.join(".mtrm").join("state.toml"));
        assert_eq!(paths.data_dir(), home.join(".mtrm").as_path());
        assert_eq!(paths.state_file(), home.join(".mtrm/state.toml").as_path());
    }

    #[test]
    fn ensure_data_dir_creates_directory_when_missing() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let paths = resolve_paths_from_home(home).unwrap();

        ensure_data_dir_at(&paths).unwrap();

        assert!(paths.data_dir.is_dir());
    }

    #[test]
    fn ensure_data_dir_is_idempotent() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let paths = resolve_paths_from_home(home).unwrap();

        ensure_data_dir_at(&paths).unwrap();
        ensure_data_dir_at(&paths).unwrap();

        assert!(paths.data_dir.is_dir());
        assert_eq!(paths.state_file, paths.data_dir.join("state.toml"));
    }

    #[test]
    fn ensure_data_dir_reports_creation_errors() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let blocking_path = home.join(".mtrm");
        File::create(&blocking_path).unwrap();
        let paths = resolve_paths_from_home(home).unwrap();

        let error = ensure_data_dir_at(&paths).unwrap_err();

        match error {
            ConfigError::CreateDir { path, .. } => assert_eq!(path, blocking_path),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn resolve_paths_from_empty_home_fails() {
        let error = resolve_paths_from_home(PathBuf::new()).unwrap_err();
        assert!(matches!(error, ConfigError::HomeDirUnavailable));
    }
}
