use std::{
    fs,
    path::{Path, PathBuf},
    process,
};

use serde::Serialize;
use uuid::Uuid;

use crate::{config, job_log::JobLog};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessState {
    Running,
    Exited,
    Unknown,
}

fn logs_root() -> PathBuf {
    config::data_directory().join("logs")
}

pub fn latest_path(tab_id: Uuid) -> PathBuf {
    logs_root()
        .join(process::id().to_string())
        .join(format!("{tab_id}.json"))
}

pub fn remove_tab_log(tab_id: Uuid) -> Result<(), String> {
    let history = history_path(tab_id);
    if history.exists() {
        fs::remove_file(&history)
            .map_err(|error| format!("Could not remove {}: {error}", history.display()))?;
    }
    let path = latest_path(tab_id);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Could not remove job log {}: {error}",
            path.display()
        )),
    }
}

/// Start a fresh log area for this process and clean only other process
/// directories whose owners are known to have exited.
pub fn prepare_session() -> Result<Vec<String>, String> {
    let root = logs_root();
    let current_pid = process::id();
    fs::create_dir_all(&root).map_err(|error| {
        format!(
            "Could not create job log folder {}: {error}",
            root.display()
        )
    })?;

    let current_dir = root.join(current_pid.to_string());
    remove_pid_directory(&current_dir)?;
    fs::create_dir(&current_dir).map_err(|error| {
        format!(
            "Could not initialize this process's job log folder {}: {error}",
            current_dir.display()
        )
    })?;

    let mut warnings = Vec::new();
    let entries = fs::read_dir(&root).map_err(|error| {
        format!(
            "Could not inspect job log folder {}: {error}",
            root.display()
        )
    })?;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                warnings.push(format!("Could not inspect a job log entry: {error}"));
                continue;
            }
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                warnings.push(format!(
                    "Could not inspect {}: {error}",
                    entry.path().display()
                ));
                continue;
            }
        };
        if !file_type.is_dir() {
            continue;
        }
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == 0 || pid == current_pid {
            continue;
        }

        match process_state(pid) {
            ProcessState::Running => {}
            ProcessState::Exited => {
                if let Err(error) = remove_pid_directory(&entry.path()) {
                    warnings.push(error);
                }
            }
            ProcessState::Unknown => warnings.push(format!(
                "Could not determine whether process {pid} is running; its job logs were left in {}.",
                entry.path().display()
            )),
        }
    }
    Ok(warnings)
}

pub fn cleanup_session() -> Result<(), String> {
    remove_pid_directory(&logs_root().join(process::id().to_string()))
}

fn remove_pid_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            fs::remove_dir_all(path).map_err(|error| {
                format!(
                    "Could not remove job log folder {}: {error}",
                    path.display()
                )
            })
        }
        Ok(_) => Err(format!(
            "Could not initialize job log folder {}; it is not a directory.",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Could not inspect job log folder {}: {error}",
            path.display()
        )),
    }
}

#[cfg(windows)]
fn process_state(pid: u32) -> ProcessState {
    use std::ffi::c_void;

    type Handle = *mut c_void;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const ERROR_INVALID_PARAMETER: u32 = 87;
    const STILL_ACTIVE: u32 = 259;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "OpenProcess"]
        fn open_process(access: u32, inherit_handle: i32, process_id: u32) -> Handle;
        #[link_name = "GetExitCodeProcess"]
        fn get_exit_code_process(process: Handle, exit_code: *mut u32) -> i32;
        #[link_name = "CloseHandle"]
        fn close_handle(object: Handle) -> i32;
        #[link_name = "GetLastError"]
        fn get_last_error() -> u32;
    }

    // Query-only access distinguishes running, exited, and inaccessible
    // processes without touching their job log files.
    let handle = unsafe { open_process(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return if unsafe { get_last_error() } == ERROR_INVALID_PARAMETER {
            ProcessState::Exited
        } else {
            ProcessState::Unknown
        };
    }

    let mut exit_code = 0;
    let queried = unsafe { get_exit_code_process(handle, &mut exit_code) } != 0;
    unsafe {
        close_handle(handle);
    }
    if !queried {
        ProcessState::Unknown
    } else if exit_code == STILL_ACTIVE {
        ProcessState::Running
    } else {
        ProcessState::Exited
    }
}

#[cfg(unix)]
fn process_state(pid: u32) -> ProcessState {
    use std::os::raw::c_int;

    const ESRCH: i32 = 3;
    const EPERM: i32 = 1;

    unsafe extern "C" {
        fn kill(pid: c_int, signal: c_int) -> c_int;
    }

    let Ok(pid) = i32::try_from(pid) else {
        return ProcessState::Unknown;
    };
    if unsafe { kill(pid, 0) } == 0 {
        return ProcessState::Running;
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(ESRCH) => ProcessState::Exited,
        Some(EPERM) => ProcessState::Running,
        _ => ProcessState::Unknown,
    }
}

#[cfg(not(any(windows, unix)))]
fn process_state(_pid: u32) -> ProcessState {
    ProcessState::Unknown
}

pub fn save(path: &Path, log: &JobLog) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Job log path has no parent folder".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    let content = serde_json::to_vec_pretty(log)
        .map_err(|error| format!("Could not encode job log: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, content)
        .map_err(|error| format!("Could not write {}: {error}", temporary.display()))?;
    config::replace_file(&temporary, path)
        .map_err(|error| format!("Could not save {}: {error}", path.display()))
}

// Persistent profile-owned history is separate from disposable legacy PID logs.
fn history_path(tab_id: Uuid) -> PathBuf {
    logs_root().join("history").join(format!("{tab_id}.json"))
}

pub fn load_history(tab_id: Uuid) -> Result<Vec<JobLog>, String> {
    let path = history_path(tab_id);
    match fs::read(&path) {
        Ok(bytes) => {
            let logs: Vec<JobLog> = serde_json::from_slice(&bytes)
                .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
            if logs.iter().any(|log| log.tab_id != tab_id) {
                return Err(format!("Profile mismatch in {}", path.display()));
            }
            Ok(logs)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!("Could not read {}: {error}", path.display())),
    }
}

pub fn save_history(tab_id: Uuid, logs: &[JobLog]) -> Result<(), String> {
    write_history(tab_id, logs)
}

/// Save the new latest run and a bounded view of the prior history without
/// cloning the existing JobLog records.
pub fn save_history_with_latest(
    tab_id: Uuid,
    latest: &JobLog,
    previous: &[JobLog],
    limit: usize,
) -> Result<(), String> {
    let mut logs = Vec::with_capacity(limit.min(previous.len() + 1));
    if limit > 0 {
        logs.push(latest);
        logs.extend(previous.iter().take(limit - 1));
    }
    write_history(tab_id, &logs)
}

fn write_history<T: Serialize + ?Sized>(tab_id: Uuid, logs: &T) -> Result<(), String> {
    let path = history_path(tab_id);
    fs::create_dir_all(path.parent().expect("history parent"))
        .map_err(|error| format!("Could not create log history folder: {error}"))?;
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let result = (|| {
        use std::io::Write;
        let bytes = serde_json::to_vec_pretty(logs).map_err(|error| error.to_string())?;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| error.to_string())?;
        drop(file);
        config::replace_file(&temporary, &path).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| format!("Could not save {}: {error}", path.display()))
}
