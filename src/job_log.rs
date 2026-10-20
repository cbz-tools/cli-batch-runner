use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    #[default]
    Pending,
    Running,
    Success,
    Returned,
    Error,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileJobLog {
    #[serde(default)]
    pub file_id: Uuid,
    pub file_name: String,
    pub path: String,
    pub command: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub working_directory: String,
    pub status: FileStatus,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    #[serde(default)]
    pub duration_ms: Option<u128>,
    pub error: Option<String>,
}

impl FileJobLog {
    pub fn pending(
        file_id: Uuid,
        file_name: String,
        path: String,
        command: String,
        arguments: Vec<String>,
        working_directory: String,
    ) -> Self {
        Self {
            file_id,
            file_name,
            path,
            command,
            arguments,
            working_directory,
            status: FileStatus::Pending,
            started_at: None,
            finished_at: None,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobLog {
    pub job_id: Uuid,
    pub tab_id: Uuid,
    pub tab_name: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u128>,
    pub aborted: bool,
    #[serde(default)]
    pub stop_reason: Option<String>,
    pub files: Vec<FileJobLog>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct JobResultCounts {
    pub exit_codes: BTreeMap<i32, usize>,
    pub errors: usize,
    pub stopped: usize,
}

impl JobResultCounts {
    pub fn from_files(files: &[FileJobLog]) -> Self {
        let mut counts = Self::default();
        for file in files {
            if let Some(code) = file.exit_code {
                *counts.exit_codes.entry(code).or_default() += 1;
            } else {
                match file.status {
                    FileStatus::Error => counts.errors += 1,
                    FileStatus::Stopped => counts.stopped += 1,
                    FileStatus::Pending
                    | FileStatus::Running
                    | FileStatus::Success
                    | FileStatus::Returned => {}
                }
            }
        }
        counts
    }

    pub fn from_statuses(files: impl IntoIterator<Item = (FileStatus, Option<i32>)>) -> Self {
        let mut counts = Self::default();
        for (status, exit_code) in files {
            match status {
                FileStatus::Success => *counts.exit_codes.entry(0).or_default() += 1,
                FileStatus::Returned => {
                    if let Some(code) = exit_code {
                        *counts.exit_codes.entry(code).or_default() += 1;
                    }
                }
                FileStatus::Error => counts.errors += 1,
                FileStatus::Stopped => counts.stopped += 1,
                FileStatus::Pending | FileStatus::Running => {}
            }
        }
        counts
    }

    pub fn display_parts(
        &self,
        success_ret0_label: &str,
        error_label: &str,
        stopped_label: &str,
    ) -> Vec<String> {
        let mut parts = Vec::new();
        for (code, count) in &self.exit_codes {
            if *code == 0 {
                parts.push(format!("{success_ret0_label}: {count}"));
            } else {
                parts.push(format!("Ret{code}: {count}"));
            }
        }
        if self.errors > 0 {
            parts.push(format!("{error_label}: {}", self.errors));
        }
        if self.stopped > 0 {
            parts.push(format!("{stopped_label}: {}", self.stopped));
        }
        parts
    }
}

pub fn timestamp_now() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S %:z").to_string()
}
