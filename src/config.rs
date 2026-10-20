use std::{
    env, fs,
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_THEME_NAME: &str = "Default Light";
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn default_theme_name() -> String {
    DEFAULT_THEME_NAME.to_owned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NonZeroExit {
    #[default]
    Continue,
    Stop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliProfile {
    pub id: Uuid,
    pub display_name: String,
    pub exe_path: String,
    pub args_template: String,
    pub working_directory: String,
    #[serde(default)]
    pub on_non_zero_exit: NonZeroExit,
    #[serde(default)]
    pub keep_job_log_history: bool,
}

impl CliProfile {
    pub fn new(index: usize) -> Self {
        Self {
            id: Uuid::new_v4(),
            display_name: format!("CLI {index}"),
            exe_path: String::new(),
            args_template: String::new(),
            working_directory: String::new(),
            on_non_zero_exit: NonZeroExit::Continue,
            keep_job_log_history: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default = "default_theme_name")]
    pub theme: String,
    pub tabs: Vec<CliProfile>,
    pub selected_tab_id: Option<Uuid>,
    pub window_x: Option<f32>,
    pub window_y: Option<f32>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let first = CliProfile::new(1);
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            theme: default_theme_name(),
            selected_tab_id: Some(first.id),
            tabs: vec![first],
            window_x: None,
            window_y: None,
        }
    }
}

impl AppConfig {
    pub fn normalize(&mut self) {
        if self.tabs.is_empty() {
            self.selected_tab_id = None;
            return;
        }
        if !self
            .selected_tab_id
            .is_some_and(|selected| self.tabs.iter().any(|tab| tab.id == selected))
        {
            self.selected_tab_id = self.tabs.first().map(|tab| tab.id);
        }
    }
}

pub fn data_directory() -> PathBuf {
    let base = env::var_os("APPDATA")
        .or_else(|| env::var_os("LOCALAPPDATA"))
        .map(PathBuf::from)
        .or_else(|| {
            env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(PathBuf::from))
        })
        .unwrap_or_else(env::temp_dir);
    base.join("cli-batch-runner")
}

pub fn config_path() -> PathBuf {
    data_directory().join("cli-batch-runner.toml")
}

pub fn load() -> (AppConfig, Option<String>, bool) {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(contents) => {
            let parsed_value = match toml::from_str::<toml::Value>(&contents) {
                Ok(value) => value,
                Err(error) => {
                    return (
                        AppConfig::default(),
                        Some(format!(
                            "Could not read {}: {error}. Configuration saving is disabled to protect this file.",
                            path.display()
                        )),
                        false,
                    );
                }
            };
            let schema_version = match parsed_value.get("schema_version") {
                None => 0,
                Some(value) => match value
                    .as_integer()
                    .and_then(|version| u32::try_from(version).ok())
                {
                    Some(version) => version,
                    None => {
                        return (
                            AppConfig::default(),
                            Some(format!(
                                "Invalid configuration schema_version {} in {}. Configuration saving is disabled to protect this file.",
                                value,
                                path.display()
                            )),
                            false,
                        );
                    }
                },
            };
            if schema_version > CURRENT_SCHEMA_VERSION {
                return (
                    AppConfig::default(),
                    Some(format!(
                        "Unsupported configuration schema version {} in {}. Configuration saving is disabled to protect this file.",
                        schema_version,
                        path.display()
                    )),
                    false,
                );
            }

            match toml::from_str::<AppConfig>(&contents) {
                Ok(mut config) => {
                    config.schema_version = CURRENT_SCHEMA_VERSION;
                    config.normalize();
                    (config, None, true)
                }
                Err(error) => (
                    AppConfig::default(),
                    Some(format!(
                        "Could not read {}: {error}. Configuration saving is disabled to protect this file.",
                        path.display()
                    )),
                    false,
                ),
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            (AppConfig::default(), None, true)
        }
        Err(error) => (
            AppConfig::default(),
            Some(format!(
                "Could not read {}: {error}. Configuration saving is disabled to protect this file.",
                path.display()
            )),
            false,
        ),
    }
}

pub fn save(config: &AppConfig) -> Result<(), String> {
    let path = config_path();
    let parent = path
        .parent()
        .ok_or_else(|| "Configuration path has no parent folder".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    let mut config = config.clone();
    config.schema_version = CURRENT_SCHEMA_VERSION;
    let contents = toml::to_string_pretty(&config)
        .map_err(|error| format!("Could not serialize configuration: {error}"))?;
    let (temporary_path, mut temporary_file) = create_temporary_file(&path)?;
    let write_result = temporary_file
        .write_all(contents.as_bytes())
        .and_then(|()| temporary_file.sync_all());
    drop(temporary_file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("Could not save {}: {error}", path.display()));
    }

    if let Err(error) = replace_file(&temporary_path, &path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("Could not save {}: {error}", path.display()));
    }
    Ok(())
}

fn create_temporary_file(path: &std::path::Path) -> Result<(PathBuf, fs::File), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Configuration path has no parent folder".to_owned())?;
    let file_name = path
        .file_name()
        .ok_or_else(|| "Configuration path has no file name".to_owned())?;

    for _ in 0..100 {
        let sequence = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut temporary_name = file_name.to_os_string();
        temporary_name.push(format!(".{}.{}.tmp", std::process::id(), sequence));
        let temporary_path = parent.join(temporary_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => return Ok((temporary_path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Could not create temporary configuration file {}: {error}",
                    temporary_path.display()
                ));
            }
        }
    }
    Err("Could not allocate a temporary configuration file".to_owned())
}

#[cfg(windows)]
pub(crate) fn replace_file(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
pub(crate) fn replace_file(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> std::io::Result<()> {
    fs::rename(source, destination)
}
