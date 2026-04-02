//! Чтение и запись снимка состояния на диск.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use mtrm_config::{ensure_data_dir, resolve_paths};
use mtrm_session::SessionSnapshot;
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};
use thiserror::Error;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);
const STATE_VERSION: &str = "0.1.0";
const LEGACY_YAML_STATE_VERSION: &str = "0.0.1";
const LEGACY_STATE_FILE_NAME: &str = "state.toml";

#[derive(Debug, Error)]
pub enum StateError {
    #[error("failed to resolve mtrm paths")]
    Config(String),
    #[error("failed to serialize snapshot")]
    Serialize(String),
    #[error("failed to deserialize snapshot")]
    Deserialize(String),
    #[error("failed to read state file")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write state file")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub fn load_state() -> Result<Option<SessionSnapshot>, StateError> {
    let paths = resolve_paths().map_err(|error| StateError::Config(error.to_string()))?;
    load_state_with_legacy_fallback(paths.state_file())
}

pub fn save_state(snapshot: &SessionSnapshot) -> Result<(), StateError> {
    let paths = ensure_data_dir().map_err(|error| StateError::Config(error.to_string()))?;
    save_state_to_path(paths.state_file(), snapshot)
}

pub fn load_state_from_path(path: &Path) -> Result<Option<SessionSnapshot>, StateError> {
    load_single_state_file(path)
}

fn load_state_with_legacy_fallback(path: &Path) -> Result<Option<SessionSnapshot>, StateError> {
    match load_single_state_file(path) {
        Ok(Some(snapshot)) => Ok(Some(snapshot)),
        Ok(None) => load_single_state_file(&legacy_state_path_for(path)),
        Err(error) => Err(error),
    }
}

fn load_single_state_file(path: &Path) -> Result<Option<SessionSnapshot>, StateError> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(StateError::Read {
                path: path.to_path_buf(),
                source: error,
            });
        }
    };

    let snapshot = match path.extension().and_then(|ext| ext.to_str()) {
        Some("yaml") | Some("yml") => deserialize_yaml_state(&content)?,
        _ => deserialize_legacy_toml_state(&content)?,
    };
    Ok(Some(snapshot))
}

pub fn save_state_to_path(path: &Path, snapshot: &SessionSnapshot) -> Result<(), StateError> {
    save_state_to_path_with_sync(path, snapshot, &RealSyncOps)
}

fn save_state_to_path_with_sync(
    path: &Path,
    snapshot: &SessionSnapshot,
    sync_ops: &dyn SyncOps,
) -> Result<(), StateError> {
    let serialized = serialize_yaml_state(snapshot)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| StateError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let temp_path = temporary_path_for(path);
    fs::write(&temp_path, serialized).map_err(|source| StateError::Write {
        path: temp_path.clone(),
        source,
    })?;
    sync_ops
        .sync_file(&temp_path)
        .map_err(|source| StateError::Write {
            path: temp_path.clone(),
            source,
        })?;

    fs::rename(&temp_path, path).map_err(|source| StateError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    if let Some(parent) = path.parent() {
        sync_ops
            .sync_dir(parent)
            .map_err(|source| StateError::Write {
                path: parent.to_path_buf(),
                source,
            })?;
    }

    Ok(())
}

trait SyncOps {
    fn sync_file(&self, path: &Path) -> Result<(), std::io::Error>;
    fn sync_dir(&self, path: &Path) -> Result<(), std::io::Error>;
}

struct RealSyncOps;

impl SyncOps for RealSyncOps {
    fn sync_file(&self, path: &Path) -> Result<(), std::io::Error> {
        fs::OpenOptions::new().read(true).open(path)?.sync_all()
    }

    fn sync_dir(&self, path: &Path) -> Result<(), std::io::Error> {
        fs::File::open(path)?.sync_all()
    }
}

fn temporary_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("state.yaml");
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    path.with_file_name(format!(".{file_name}.{pid}.{now}.{counter}.tmp"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedStateFile {
    version: String,
    #[serde(flatten)]
    snapshot: JsonMap<String, JsonValue>,
}

impl PersistedStateFile {
    fn from_snapshot(snapshot: &SessionSnapshot) -> Self {
        let JsonValue::Object(snapshot) =
            serde_json::to_value(snapshot).expect("session snapshot must serialize into an object")
        else {
            unreachable!("session snapshot must serialize into a JSON object");
        };
        Self {
            version: STATE_VERSION.to_owned(),
            snapshot,
        }
    }

    fn into_snapshot(self) -> Result<SessionSnapshot, StateError> {
        if self.version != STATE_VERSION && self.version != LEGACY_YAML_STATE_VERSION {
            return Err(StateError::Deserialize(format!(
                "unsupported state version: {}",
                self.version
            )));
        }
        // Для legacy YAML 0.0.1 отдельный переходник на уровне state-файла не нужен:
        // нижележащий layout-layer умеет читать старую бинарную форму split-узлов
        // (`first`/`second`) и сам приводит ее к новой n-арной модели.
        serde_json::from_value(JsonValue::Object(self.snapshot))
            .map_err(|error| StateError::Deserialize(error.to_string()))
    }
}

fn serialize_yaml_state(snapshot: &SessionSnapshot) -> Result<String, StateError> {
    serde_yaml::to_string(&PersistedStateFile::from_snapshot(snapshot))
        .map_err(|error| StateError::Serialize(error.to_string()))
}

fn deserialize_yaml_state(content: &str) -> Result<SessionSnapshot, StateError> {
    let persisted: PersistedStateFile =
        serde_yaml::from_str(content).map_err(|error| StateError::Deserialize(error.to_string()))?;
    persisted.into_snapshot()
}

fn deserialize_legacy_toml_state(content: &str) -> Result<SessionSnapshot, StateError> {
    toml::from_str(content).map_err(|error| StateError::Deserialize(error.to_string()))
}

fn legacy_state_path_for(path: &Path) -> PathBuf {
    path.with_file_name(LEGACY_STATE_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtrm_core::{PaneId, TabId};
    use mtrm_layout::LayoutTree;
    use mtrm_session::{PaneSnapshot, TabSnapshot};
    use serial_test::serial;
    use tempfile::tempdir;

    fn sample_legacy_toml(snapshot: &SessionSnapshot) -> String {
        toml::to_string_pretty(snapshot).unwrap()
    }

    struct FailingFileSync;

    impl SyncOps for FailingFileSync {
        fn sync_file(&self, _path: &Path) -> Result<(), std::io::Error> {
            Err(std::io::Error::other("sync file failed"))
        }

        fn sync_dir(&self, _path: &Path) -> Result<(), std::io::Error> {
            Ok(())
        }
    }

    fn sample_snapshot() -> SessionSnapshot {
        let layout = LayoutTree::new(PaneId::new(10)).to_snapshot();
        SessionSnapshot {
            tabs: vec![TabSnapshot {
                id: TabId::new(1),
                title: "main".to_owned(),
                layout,
                panes: vec![PaneSnapshot {
                    id: PaneId::new(10),
                    cwd: PathBuf::from("/tmp"),
                    title: "pane-10".to_owned(),
                }],
                active_pane: PaneId::new(10),
            }],
            active_tab: TabId::new(1),
        }
    }

    #[test]
    fn load_state_from_path_returns_none_for_missing_file() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("missing.yaml");

        let result = load_state_from_path(&path).unwrap();

        assert_eq!(result, None);
    }

    #[test]
    fn save_and_load_yaml_roundtrip_without_loss() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("state.yaml");
        let snapshot = sample_snapshot();

        save_state_to_path(&path, &snapshot).unwrap();
        let loaded = load_state_from_path(&path).unwrap();

        assert_eq!(loaded, Some(snapshot));
    }

    #[test]
    fn save_state_writes_yaml_with_version() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("state.yaml");
        let snapshot = sample_snapshot();

        save_state_to_path(&path, &snapshot).unwrap();
        let written = fs::read_to_string(&path).unwrap();

        assert!(written.contains("version: 0.1.0"));
    }

    #[test]
    fn load_state_prefers_yaml_over_legacy_toml() {
        let temp = tempdir().unwrap();
        let yaml_path = temp.path().join("state.yaml");
        let toml_path = temp.path().join("state.toml");

        let yaml_snapshot = sample_snapshot();
        let mut toml_snapshot = sample_snapshot();
        toml_snapshot.active_tab = TabId::new(99);

        save_state_to_path(&yaml_path, &yaml_snapshot).unwrap();
        fs::write(&toml_path, sample_legacy_toml(&toml_snapshot)).unwrap();

        let loaded = load_state_with_legacy_fallback(&yaml_path).unwrap();

        assert_eq!(loaded, Some(yaml_snapshot));
    }

    #[test]
    fn load_state_falls_back_to_legacy_toml_when_yaml_is_missing() {
        let temp = tempdir().unwrap();
        let yaml_path = temp.path().join("state.yaml");
        let toml_path = temp.path().join("state.toml");
        let snapshot = sample_snapshot();

        fs::write(&toml_path, sample_legacy_toml(&snapshot)).unwrap();

        let loaded = load_state_with_legacy_fallback(&yaml_path).unwrap();

        assert_eq!(loaded, Some(snapshot));
    }

    #[test]
    fn yaml_without_pane_title_still_loads() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("state.yaml");
        fs::write(
            &path,
            "version: 0.0.1\nactive_tab: 1\ntabs:\n  - id: 1\n    title: main\n    layout:\n      root:\n        Pane:\n          pane_id: 10\n      focused_pane: 10\n    panes:\n      - id: 10\n        cwd: /tmp\n    active_pane: 10\n",
        )
        .unwrap();

        let loaded = load_state_from_path(&path).unwrap().unwrap();

        assert_eq!(loaded.tabs[0].panes[0].title, "");
    }

    #[test]
    fn damaged_yaml_file_returns_deserialize_error() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("state.yaml");
        fs::write(&path, "version: [not valid").unwrap();

        let error = load_state_from_path(&path).unwrap_err();

        assert!(matches!(error, StateError::Deserialize(_)));
    }

    #[test]
    fn unsupported_yaml_version_returns_deserialize_error() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("state.yaml");
        let yaml = serialize_yaml_state(&sample_snapshot()).unwrap();
        fs::write(&path, yaml.replacen("0.1.0", "0.0.2", 1)).unwrap();

        let error = load_state_from_path(&path).unwrap_err();

        assert!(matches!(error, StateError::Deserialize(_)));
    }

    #[test]
    fn atomic_write_leaves_final_file_in_valid_state() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("state.yaml");
        let snapshot = sample_snapshot();

        save_state_to_path(&path, &snapshot).unwrap();

        assert!(path.is_file());

        let loaded = load_state_from_path(&path).unwrap();
        assert_eq!(loaded, Some(snapshot));
    }

    #[test]
    fn fsync_file_error_is_reported_as_write_error() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("state.yaml");
        let snapshot = sample_snapshot();

        let error = save_state_to_path_with_sync(&path, &snapshot, &FailingFileSync).unwrap_err();

        assert!(matches!(error, StateError::Write { .. }));
    }

    struct FailingDirSync;

    impl SyncOps for FailingDirSync {
        fn sync_file(&self, _path: &Path) -> Result<(), std::io::Error> {
            Ok(())
        }

        fn sync_dir(&self, _path: &Path) -> Result<(), std::io::Error> {
            Err(std::io::Error::other("sync dir failed"))
        }
    }

    #[test]
    fn fsync_dir_error_is_reported_as_write_error() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("state.yaml");
        let snapshot = sample_snapshot();

        let error = save_state_to_path_with_sync(&path, &snapshot, &FailingDirSync).unwrap_err();

        assert!(matches!(error, StateError::Write { .. }));
    }

    #[test]
    fn temporary_path_is_not_fixed_single_name() {
        let path = PathBuf::from("/tmp/state.yaml");
        let first = temporary_path_for(&path);
        let second = temporary_path_for(&path);

        assert_ne!(
            first, second,
            "temporary file path must be unique per save attempt"
        );
    }

    #[test]
    fn write_error_display_is_sanitized_but_debug_keeps_path() {
        let error = StateError::Write {
            path: PathBuf::from("/tmp/secret/state.yaml"),
            source: std::io::Error::other("permission denied"),
        };

        let display = error.to_string();
        let debug = format!("{error:?}");

        assert!(!display.contains("/tmp/secret"));
        assert!(!display.contains("permission denied"));
        assert!(debug.contains("/tmp/secret"));
    }

    #[test]
    #[serial]
    fn save_state_creates_service_directory() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let snapshot = sample_snapshot();

        // SAFETY: this test is serialized and restores the environment before exit.
        let previous_home = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("HOME", &home);
        }

        let result = save_state(&snapshot);

        if let Some(previous_home) = previous_home {
            // SAFETY: this test is serialized and restores the environment before exit.
            unsafe {
                std::env::set_var("HOME", previous_home);
            }
        } else {
            // SAFETY: this test is serialized and restores the environment before exit.
            unsafe {
                std::env::remove_var("HOME");
            }
        }

        result.unwrap();
        assert!(home.join(".mtrm").is_dir());
        assert!(home.join(".mtrm").join("state.yaml").is_file());
    }
}
