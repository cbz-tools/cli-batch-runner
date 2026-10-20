use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

use crate::{
    cli::{ResolvedProfilePaths, arguments_for_file, display_argument, display_path},
    config::{CliProfile, NonZeroExit},
    job_log::{FileJobLog, FileStatus, timestamp_now},
};

#[derive(Debug, Clone)]
pub struct RunnerFile {
    pub id: Uuid,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct JobSnapshot {
    pub profile: CliProfile,
    pub paths: ResolvedProfilePaths,
    pub files: Vec<RunnerFile>,
}

#[derive(Debug)]
pub enum RunnerEvent {
    FileStarted {
        index: usize,
        entry: FileJobLog,
    },
    FileFinished {
        index: usize,
        entry: FileJobLog,
    },
    Finished {
        aborted: bool,
        finished_at: String,
        stop_reason: Option<String>,
    },
}

pub fn pending_log_entry(
    profile: &CliProfile,
    paths: &ResolvedProfilePaths,
    file_id: Uuid,
    input: &Path,
) -> FileJobLog {
    let path = display_path(input);
    let file_name = input
        .file_name()
        .unwrap_or(input.as_os_str())
        .to_string_lossy()
        .into_owned();
    let (arguments, error) = match arguments_for_file(&profile.args_template, input) {
        Ok(arguments) => (
            arguments
                .into_iter()
                .map(|argument| display_argument(&argument, input))
                .collect(),
            None,
        ),
        Err(error) => (Vec::new(), Some(error)),
    };
    let mut entry = FileJobLog::pending(
        file_id,
        file_name,
        path,
        display_path(&paths.executable),
        arguments,
        display_path(&paths.working_directory),
    );
    entry.error = error;
    entry
}

pub fn run_job(snapshot: JobSnapshot, stop: Arc<AtomicBool>, tx: Sender<RunnerEvent>) {
    let mut aborted = false;
    let mut stop_reason = None;
    for (index, file) in snapshot.files.iter().enumerate() {
        if stop.load(Ordering::Relaxed) {
            aborted = true;
            stop_reason = Some("Stopped by user.".to_owned());
            break;
        }
        let started_at = timestamp_now();
        let started = Instant::now();
        let mut entry = pending_log_entry(&snapshot.profile, &snapshot.paths, file.id, &file.path);
        entry.status = FileStatus::Running;
        entry.started_at = Some(started_at);
        let _ = tx.send(RunnerEvent::FileStarted {
            index,
            entry: entry.clone(),
        });
        let (entry, was_stopped) = execute_file(
            &snapshot.profile,
            &snapshot.paths,
            &file.path,
            entry,
            started,
            &stop,
        );
        let stop_on_return = snapshot.profile.on_non_zero_exit == NonZeroExit::Stop
            && entry.status == FileStatus::Returned;
        let _ = tx.send(RunnerEvent::FileFinished { index, entry });
        if was_stopped || stop_on_return {
            aborted = true;
            stop_reason = Some(
                if was_stopped {
                    "Stopped by user."
                } else {
                    "Stopped on non-zero exit."
                }
                .to_owned(),
            );
            break;
        }
    }
    let _ = tx.send(RunnerEvent::Finished {
        aborted,
        stop_reason,
        finished_at: timestamp_now(),
    });
}

fn execute_file(
    profile: &CliProfile,
    paths: &ResolvedProfilePaths,
    input: &Path,
    mut entry: FileJobLog,
    started: Instant,
    stop: &AtomicBool,
) -> (FileJobLog, bool) {
    if let Some(error) = entry.error.take() {
        return fail_entry(entry, error, false, started);
    }
    let arguments = match arguments_for_file(&profile.args_template, input) {
        Ok(arguments) => arguments,
        Err(error) => return fail_entry(entry, error, false, started),
    };
    let mut command = Command::new(&paths.executable);
    command
        .args(&arguments)
        .current_dir(&paths.working_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return fail_entry(
                entry,
                format!("Could not start process: {error}"),
                false,
                started,
            );
        }
    };
    let stdout = child.stdout.take().map(read_pipe);
    let stderr = child.stderr.take().map(read_pipe);
    let (exit_code, stopped, wait_error) = loop {
        if stop.load(Ordering::Relaxed) {
            match child.kill() {
                Ok(()) => {
                    let status = child.wait().ok();
                    break (status.and_then(|status| status.code()), true, None);
                }
                Err(error) => match child.try_wait() {
                    Ok(Some(status)) => break (status.code(), false, None),
                    Ok(None) => {
                        let _ = child.kill();
                        let status = child.wait().ok();
                        break (
                            status.and_then(|status| status.code()),
                            true,
                            Some(format!("Could not stop child process cleanly: {error}")),
                        );
                    }
                    Err(wait_error) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break (
                            None,
                            true,
                            Some(format!("Could not stop child process: {wait_error}")),
                        );
                    }
                },
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => break (status.code(), false, None),
            Ok(None) => thread::sleep(Duration::from_millis(30)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break (
                    None,
                    false,
                    Some(format!("Could not wait for child process: {error}")),
                );
            }
        }
    };
    let stdout = join_pipe(stdout);
    let stderr = join_pipe(stderr);
    entry.stdout = stdout.0;
    entry.stderr = stderr.0;
    if let Some(error) = stdout.1.or(stderr.1) {
        entry.error = Some(error);
    }
    if let Some(error) = wait_error {
        entry.error = Some(match entry.error.take() {
            Some(previous) => format!("{previous}; {error}"),
            None => error,
        });
    }
    entry.finished_at = Some(timestamp_now());
    entry.duration_ms = Some(started.elapsed().as_millis());
    if stopped {
        entry.status = FileStatus::Stopped;
        entry.exit_code = None;
        entry.error = Some(match entry.error.take() {
            Some(previous) => format!("Stopped by user. {previous}"),
            None => "Stopped by user.".to_owned(),
        });
    } else {
        entry.exit_code = exit_code;
        entry.status = match exit_code {
            Some(0) => FileStatus::Success,
            Some(_) => FileStatus::Returned,
            None => FileStatus::Error,
        };
    }
    (entry, stopped)
}

fn fail_entry(
    mut entry: FileJobLog,
    error: String,
    stopped: bool,
    started: Instant,
) -> (FileJobLog, bool) {
    entry.status = if stopped {
        FileStatus::Stopped
    } else {
        FileStatus::Error
    };
    entry.finished_at = Some(timestamp_now());
    entry.duration_ms = Some(started.elapsed().as_millis());
    entry.exit_code = None;
    entry.error = Some(error);
    (entry, stopped)
}

fn read_pipe<R: Read + Send + 'static>(
    mut pipe: R,
) -> thread::JoinHandle<std::io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes)?;
        Ok(bytes)
    })
}

fn join_pipe(
    handle: Option<thread::JoinHandle<std::io::Result<Vec<u8>>>>,
) -> (String, Option<String>) {
    let Some(handle) = handle else {
        return (String::new(), None);
    };
    match handle.join() {
        Ok(Ok(bytes)) => (String::from_utf8_lossy(&bytes).into_owned(), None),
        Ok(Err(error)) => (
            String::new(),
            Some(format!("Could not read process output: {error}")),
        ),
        Err(_) => (
            String::new(),
            Some("Process output reader stopped unexpectedly.".to_owned()),
        ),
    }
}
