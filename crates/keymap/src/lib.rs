use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use mtrm_config::{MtrmPaths, ensure_data_dir};
use serde::Deserialize;
use thiserror::Error;

const KEYMAP_FILE_NAME: &str = "keymap.toml";
const DEFAULT_KEYMAP_TOML: &str = include_str!("../default_keymap.toml");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    pub copy: BTreeSet<char>,
    pub paste: BTreeSet<char>,
    pub interrupt: BTreeSet<char>,
    pub close_pane: BTreeSet<char>,
    pub new_tab: BTreeSet<char>,
    pub close_tab: BTreeSet<char>,
    pub rename_tab: BTreeSet<char>,
    pub rename_pane: BTreeSet<char>,
    pub quit: BTreeSet<char>,
    pub previous_tab: BTreeSet<char>,
    pub next_tab: BTreeSet<char>,
}

impl Keymap {
    pub fn from_toml_str(source: &str) -> Result<Self, KeymapError> {
        let parsed: KeymapFile =
            toml::from_str(source).map_err(|source| KeymapError::Parse { source })?;
        parsed.try_into()
    }

    pub fn matches_copy(&self, ch: char) -> bool {
        self.copy.contains(&ch)
    }

    pub fn matches_paste(&self, ch: char) -> bool {
        self.paste.contains(&ch)
    }

    pub fn matches_interrupt(&self, ch: char) -> bool {
        self.interrupt.contains(&ch)
    }

    pub fn matches_close_pane(&self, ch: char) -> bool {
        self.close_pane.contains(&ch)
    }

    pub fn matches_new_tab(&self, ch: char) -> bool {
        self.new_tab.contains(&ch)
    }

    pub fn matches_close_tab(&self, ch: char) -> bool {
        self.close_tab.contains(&ch)
    }

    pub fn matches_rename_tab(&self, ch: char) -> bool {
        self.rename_tab.contains(&ch)
    }

    pub fn matches_rename_pane(&self, ch: char) -> bool {
        self.rename_pane.contains(&ch)
    }

    pub fn matches_quit(&self, ch: char) -> bool {
        self.quit.contains(&ch)
    }

    pub fn matches_previous_tab(&self, ch: char) -> bool {
        self.previous_tab.contains(&ch)
    }

    pub fn matches_next_tab(&self, ch: char) -> bool {
        self.next_tab.contains(&ch)
    }
}

impl Default for Keymap {
    fn default() -> Self {
        Self::from_toml_str(DEFAULT_KEYMAP_TOML).expect("embedded default keymap must be valid")
    }
}

#[derive(Debug, Error)]
pub enum KeymapError {
    #[error("keymap directory setup failed")]
    Config {
        #[source]
        source: mtrm_config::ConfigError,
    },
    #[error("keymap file creation failed")]
    Create {
        #[source]
        source: std::io::Error,
    },
    #[error("keymap file read failed")]
    Read {
        #[source]
        source: std::io::Error,
    },
    #[error("keymap file write failed")]
    Write {
        #[source]
        source: std::io::Error,
    },
    #[error("keymap parse failed")]
    Parse {
        #[source]
        source: toml::de::Error,
    },
    #[error("keymap is invalid")]
    Invalid(&'static str),
}

#[derive(Debug, Deserialize)]
struct KeymapFile {
    commands: CommandsFile,
}

#[derive(Debug, Deserialize)]
struct CommandsFile {
    copy: Vec<String>,
    paste: Vec<String>,
    interrupt: Vec<String>,
    close_pane: Vec<String>,
    new_tab: Vec<String>,
    close_tab: Vec<String>,
    #[serde(default = "default_rename_tab_bindings")]
    rename_tab: Vec<String>,
    #[serde(default = "default_rename_pane_bindings")]
    rename_pane: Vec<String>,
    quit: Vec<String>,
    previous_tab: Vec<String>,
    next_tab: Vec<String>,
}

impl TryFrom<KeymapFile> for Keymap {
    type Error = KeymapError;

    fn try_from(value: KeymapFile) -> Result<Self, Self::Error> {
        Ok(Self {
            copy: parse_chars("copy", value.commands.copy)?,
            paste: parse_chars("paste", value.commands.paste)?,
            interrupt: parse_chars("interrupt", value.commands.interrupt)?,
            close_pane: parse_chars("close_pane", value.commands.close_pane)?,
            new_tab: parse_chars("new_tab", value.commands.new_tab)?,
            close_tab: parse_chars("close_tab", value.commands.close_tab)?,
            rename_tab: parse_chars("rename_tab", value.commands.rename_tab)?,
            rename_pane: parse_chars("rename_pane", value.commands.rename_pane)?,
            quit: parse_chars("quit", value.commands.quit)?,
            previous_tab: parse_chars("previous_tab", value.commands.previous_tab)?,
            next_tab: parse_chars("next_tab", value.commands.next_tab)?,
        })
    }
}

fn default_rename_tab_bindings() -> Vec<String> {
    vec!["R".to_owned(), "К".to_owned()]
}

fn default_rename_pane_bindings() -> Vec<String> {
    vec!["E".to_owned(), "У".to_owned()]
}

fn parse_chars(name: &'static str, values: Vec<String>) -> Result<BTreeSet<char>, KeymapError> {
    let mut chars = BTreeSet::new();
    for value in values {
        let mut iter = value.chars();
        let ch = iter.next().ok_or(KeymapError::Invalid(name))?;
        if iter.next().is_some() {
            return Err(KeymapError::Invalid(name));
        }
        chars.insert(ch);
    }
    if chars.is_empty() {
        return Err(KeymapError::Invalid(name));
    }
    Ok(chars)
}

pub fn default_keymap_toml() -> &'static str {
    DEFAULT_KEYMAP_TOML
}

pub fn keymap_file_path() -> Result<PathBuf, KeymapError> {
    let paths = ensure_data_dir().map_err(|source| KeymapError::Config { source })?;
    Ok(keymap_file_path_from_paths(&paths))
}

pub fn ensure_keymap_file() -> Result<PathBuf, KeymapError> {
    let paths = ensure_data_dir().map_err(|source| KeymapError::Config { source })?;
    ensure_keymap_file_at(&paths)?;
    Ok(keymap_file_path_from_paths(&paths))
}

pub fn load_keymap() -> Result<Keymap, KeymapError> {
    let path = ensure_keymap_file()?;
    load_keymap_from_path(&path)
}

pub fn load_keymap_from_path(path: &Path) -> Result<Keymap, KeymapError> {
    let contents = fs::read_to_string(path).map_err(|source| KeymapError::Read { source })?;
    Keymap::from_toml_str(&contents)
}

fn keymap_file_path_from_paths(paths: &MtrmPaths) -> PathBuf {
    paths.data_dir().join(KEYMAP_FILE_NAME)
}

fn ensure_keymap_file_at(paths: &MtrmPaths) -> Result<(), KeymapError> {
    let path = keymap_file_path_from_paths(paths);
    if path.exists() {
        return Ok(());
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|source| KeymapError::Create { source })?;
    file.write_all(DEFAULT_KEYMAP_TOML.as_bytes())
        .map_err(|source| KeymapError::Write { source })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn temp_paths(home: &Path) -> MtrmPaths {
        MtrmPaths {
            home_dir: home.to_path_buf(),
            data_dir: home.join(".mtrm"),
            state_file: home.join(".mtrm").join("state.yaml"),
        }
    }

    #[test]
    fn ensure_keymap_file_creates_default_file_when_missing() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let paths = temp_paths(&home);
        fs::create_dir_all(paths.data_dir()).unwrap();

        ensure_keymap_file_at(&paths).unwrap();

        let keymap_file = keymap_file_path_from_paths(&paths);
        assert!(keymap_file.is_file());
        assert_eq!(
            fs::read_to_string(keymap_file).unwrap(),
            default_keymap_toml()
        );
    }

    #[test]
    fn ensure_keymap_file_does_not_overwrite_existing_file() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let paths = temp_paths(&home);
        fs::create_dir_all(paths.data_dir()).unwrap();
        let keymap_file = keymap_file_path_from_paths(&paths);
        fs::write(&keymap_file, "[commands]\ncopy=['z']\npaste=['v']\ninterrupt=['x']\nclose_pane=['q']\nnew_tab=['t']\nclose_tab=['w']\nquit=['Q']\nprevious_tab=[',']\nnext_tab=['.']\n").unwrap();

        ensure_keymap_file_at(&paths).unwrap();

        assert!(
            fs::read_to_string(keymap_file)
                .unwrap()
                .contains("copy=['z']")
        );
    }

    #[test]
    fn load_keymap_from_path_reads_custom_layout_symbols() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("keymap.toml");
        fs::write(
            &path,
            "[commands]\ncopy=['λ']\npaste=['π']\ninterrupt=['ι']\nclose_pane=['κ']\nnew_tab=['ν']\nclose_tab=['χ']\nquit=['Ω']\nprevious_tab=['<']\nnext_tab=['>']\n",
        )
        .unwrap();

        let keymap = load_keymap_from_path(&path).unwrap();

        assert!(keymap.matches_new_tab('ν'));
        assert!(keymap.matches_copy('λ'));
        assert!(keymap.matches_rename_tab('R'));
        assert!(keymap.matches_rename_pane('У'));
    }

    #[test]
    fn default_keymap_covers_extra_common_layout_symbols() {
        let keymap = Keymap::default();

        assert!(keymap.matches_close_pane('a'));
        assert!(keymap.matches_close_tab('z'));
        assert!(keymap.matches_copy('ψ'));
        assert!(keymap.matches_paste('ω'));
        assert!(keymap.matches_interrupt('χ'));
        assert!(keymap.matches_new_tab('τ'));
        assert!(keymap.matches_rename_tab('К'));
        assert!(keymap.matches_rename_pane('E'));
        assert!(keymap.matches_quit(':'));
        assert!(keymap.matches_quit('X'));
        assert!(keymap.matches_quit('Ч'));
        assert!(keymap.matches_quit('Χ'));
    }

    #[test]
    fn load_keymap_from_path_rejects_invalid_toml() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("keymap.toml");
        fs::write(&path, "not = [valid").unwrap();

        let error = load_keymap_from_path(&path).unwrap_err();
        assert!(matches!(error, KeymapError::Parse { .. }));
    }
}
