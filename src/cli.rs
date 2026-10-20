use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};

use crate::config::CliProfile;

#[derive(Debug, Clone)]
pub struct ResolvedProfilePaths {
    pub executable: PathBuf,
    pub working_directory: PathBuf,
}

/// Formats a filesystem path for logs and other user-facing text without changing the path used
/// by filesystem operations or process execution.
pub fn display_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    #[cfg(windows)]
    {
        display_windows_path(&path)
    }
    #[cfg(not(windows))]
    {
        path.into_owned()
    }
}

#[cfg(windows)]
fn display_windows_path(path: &str) -> String {
    const EXTENDED_UNC_PREFIX: &str = "\\\\?\\UNC\\";
    const EXTENDED_PREFIX: &str = "\\\\?\\";

    if path
        .get(..EXTENDED_UNC_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(EXTENDED_UNC_PREFIX))
    {
        let unc_path = &path[EXTENDED_UNC_PREFIX.len()..];
        let mut parts = unc_path.split('\\');
        if parts.next().is_some_and(|server| !server.is_empty())
            && parts.next().is_some_and(|share| !share.is_empty())
        {
            return format!("\\\\{unc_path}");
        }
    }

    if let Some(drive_path) = path.strip_prefix(EXTENDED_PREFIX) {
        let bytes = drive_path.as_bytes();
        if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'\\'
        {
            return drive_path.to_owned();
        }
    }

    path.to_owned()
}

/// Normalizes only path strings that came from the input path placeholders in the argument
/// template. The actual OsString passed to Command remains untouched.
pub fn display_argument(argument: &OsStr, input: &Path) -> String {
    let mut display = argument.to_string_lossy().into_owned();
    let paths = [Some(input), input.parent()];
    for path in paths.into_iter().flatten() {
        let original = path.to_string_lossy();
        let formatted = display_path(path);
        if original != formatted {
            display = display.replace(original.as_ref(), &formatted);
        }
    }
    display
}

pub fn validate_profile(profile: &CliProfile) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if profile.display_name.trim().is_empty() {
        errors.push("Display name is required.".to_owned());
    }
    if profile.exe_path.trim().is_empty() {
        errors.push("Executable path is required.".to_owned());
    } else if let Err(error) = resolve_file(Path::new(&profile.exe_path)) {
        errors.push(error);
    }
    if !profile.working_directory.trim().is_empty()
        && let Err(error) = resolve_directory(Path::new(&profile.working_directory))
    {
        errors.push(error);
    }
    if let Err(error) = split_arguments(&profile.args_template) {
        errors.push(error);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn resolved_profile_paths(profile: &CliProfile) -> Result<ResolvedProfilePaths, String> {
    let executable = resolve_file(Path::new(&profile.exe_path))?;
    let working_directory = if profile.working_directory.trim().is_empty() {
        executable
            .parent()
            .ok_or_else(|| "Executable path has no parent folder".to_owned())?
            .to_path_buf()
    } else {
        resolve_directory(Path::new(&profile.working_directory))?
    };
    Ok(ResolvedProfilePaths {
        executable,
        working_directory,
    })
}

fn resolve_file(path: &Path) -> Result<PathBuf, String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("Could not resolve file {}: {error}", path.display()))?;
    let metadata = fs::metadata(&canonical)
        .map_err(|error| format!("Could not inspect file {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("Path is not a file: {}", path.display()));
    }
    Ok(canonical)
}

fn resolve_directory(path: &Path) -> Result<PathBuf, String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("Could not resolve directory {}: {error}", path.display()))?;
    let metadata = fs::metadata(&canonical)
        .map_err(|error| format!("Could not inspect directory {}: {error}", path.display()))?;
    if !metadata.is_dir() {
        return Err(format!("Path is not a directory: {}", path.display()));
    }
    Ok(canonical)
}

pub fn split_arguments(template: &str) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut value = String::new();
    let mut quote = None;
    let mut started = false;
    let chars: Vec<char> = template.chars().collect();
    let mut index = 0;

    while index < chars.len() {
        let ch = chars[index];
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
                started = true;
                index += 1;
                continue;
            }
            if active_quote == '"'
                && ch == '\\'
                && let Some(next @ ('"' | '\\')) = chars.get(index + 1).copied()
            {
                value.push(next);
                started = true;
                index += 2;
                continue;
            }
            value.push(ch);
            started = true;
            index += 1;
            continue;
        }

        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                started = true;
            }
            ch if ch.is_whitespace() => {
                if started {
                    result.push(std::mem::take(&mut value));
                    started = false;
                }
            }
            '\\' if matches!(chars.get(index + 1), Some('"' | '\'' | '\\')) => {
                value.push(chars[index + 1]);
                started = true;
                index += 1;
            }
            _ => {
                value.push(ch);
                started = true;
            }
        }
        index += 1;
    }

    if quote.is_some() {
        return Err("Arguments contain an unmatched quote.".to_owned());
    }
    if started {
        result.push(value);
    }
    Ok(result)
}

pub fn arguments_for_file(template: &str, input: &Path) -> Result<Vec<OsString>, String> {
    let effective_template = if template.trim().is_empty() {
        "{input}"
    } else {
        template
    };
    split_arguments(effective_template).map(|parts| {
        parts
            .into_iter()
            .map(|part| expand_argument(&part, input))
            .collect()
    })
}

fn expand_argument(template: &str, input: &Path) -> OsString {
    let input_dir = input.parent().map(Path::as_os_str).unwrap_or_default();
    let input_name = input.file_name().unwrap_or(input.as_os_str());
    let input_stem = input.file_stem().unwrap_or_default();
    let input_ext = input.extension().unwrap_or_default();

    let mut argument = OsString::new();
    let mut remaining = template;
    while let Some(open) = remaining.find('{') {
        argument.push(&remaining[..open]);
        let token_start = &remaining[open..];
        let Some(close) = token_start.find('}') else {
            argument.push(token_start);
            return argument;
        };
        let token = &token_start[..=close];
        match token {
            "{input}" => argument.push(input.as_os_str()),
            "{input_dir}" => argument.push(input_dir),
            "{input_name}" => argument.push(input_name),
            "{input_stem}" => argument.push(input_stem),
            "{input_ext}" => argument.push(input_ext),
            _ => argument.push(token),
        }
        remaining = &token_start[close + 1..];
    }
    argument.push(remaining);
    argument
}

pub fn quote_argument(value: &str) -> String {
    if !value.is_empty() && !value.chars().any(|ch| ch.is_whitespace() || ch == '"') {
        return value.to_owned();
    }
    let mut result = String::from("\"");
    let mut backslashes = 0;
    for ch in value.chars() {
        if ch == '\\' {
            backslashes += 1;
        } else if ch == '"' {
            result.push_str(&"\\".repeat(backslashes * 2 + 1));
            result.push(ch);
            backslashes = 0;
        } else {
            result.push_str(&"\\".repeat(backslashes));
            result.push(ch);
            backslashes = 0;
        }
    }
    result.push_str(&"\\".repeat(backslashes * 2));
    result.push('"');
    result
}
