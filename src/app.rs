mod dialogs;
mod header;
mod progress;
mod queue;
mod root;
mod settings;

use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

use gpui_kit::assets::IconName as LucideIcon;
use gpui_kit::{
    Context, Entity, FocusHandle, Focusable, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, PathPromptOptions, Render, ScrollHandle, StatefulInteractiveElement, Window,
    base::{
        Disableable, Scrollbar, ScrollbarMode, SelectableText, TextSelection, TextSelectionHandle,
        TextSelectionLayer, VirtualListScrollHandle, v_virtual_list,
    },
    component::ActiveTheme,
    component::IndexPath,
    component::{
        Icon, Sizable,
        button::{Button, ButtonVariants},
        input::{Input, InputEvent, InputState, Textarea, TextareaState},
        menu::{ContextMenuExt, DropdownMenu, PopupMenuItem},
        select::{SearchableVec, Select, SelectEvent, SelectState},
        tab::{Tab, TabBar},
        tag::Tag,
        tooltip::Tooltip,
    },
    div,
    prelude::*,
    px, relative, rems, size,
};
use uuid::Uuid;

use crate::{
    cli,
    config::{self, AppConfig, CliProfile, NonZeroExit},
    job_log::{FileJobLog, FileStatus, JobLog, JobResultCounts, timestamp_now},
    log_store,
    runner::{self, JobSnapshot, RunnerEvent, RunnerFile},
};

const APP_TITLE: &str = "CLI Batch Runner";
const FILE_STATUS_COLUMN_WIDTH: f32 = 124.;
const FILE_LOG_COLUMN_WIDTH: f32 = 40.;
const QUEUE_SCROLLBAR_GUTTER_WIDTH: f32 = 16.;
struct ArgumentExample {
    id: &'static str,
    title: &'static str,
    executable: &'static str,
    arguments: &'static str,
    description: &'static str,
}

#[derive(Clone, Copy)]
enum JobTarget {
    AllVisible,
    SelectedVisible,
}

const HELP_EXAMPLES_NOTE: &str = "These examples use CertUtil, a Windows built-in CLI. Choose the full executable path and adjust C:\\Windows if Windows is installed elsewhere. Copy copies the Arguments template. Paths are passed directly as arguments, without shell interpretation.\n";
const HELP_EXAMPLES: &[ArgumentExample] = &[
    ArgumentExample {
        id: "sha256",
        title: "SHA256 Hash",
        executable: r"C:\Windows\System32\certutil.exe",
        arguments: "-hashfile {input} SHA256",
        description: "Prints the SHA256 hash of each input file without changing it. View the result in its file log.\n",
    },
    ArgumentExample {
        id: "base64-export",
        title: "Base64 Export",
        executable: r"C:\Windows\System32\certutil.exe",
        arguments: r#"-encode "{input_dir}\{input_name}" "{input_dir}\{input_stem}.{input_ext}.base64.txt""#,
        description: "Exports each input as Base64 text with CertUtil's header and footer. For Sample.txt, creates Sample.txt.base64.txt beside it; for an extensionless Sample, creates Sample..base64.txt. Uses the four file-name/directory placeholders. Fails if the output exists; no force-overwrite option is used. The input is unchanged. Spaces, Japanese characters, and shell metacharacters in paths are passed as data.\n",
    },
];
const SAVE_DISABLED: &str =
    "Configuration saving is disabled to protect the existing configuration file.";
const POLL_INTERVAL: Duration = Duration::from_millis(40);

#[derive(Clone, Copy)]
enum PathDialogPurpose {
    AddFiles(Uuid),
    AddFolder(Uuid),
    Executable(Uuid),
    WorkingDirectory(Uuid),
}

impl PathDialogPurpose {
    fn options(self) -> PathPromptOptions {
        let (files, directories, multiple, prompt) = match self {
            Self::AddFiles(_) => (true, false, true, "Select files"),
            Self::AddFolder(_) => (false, true, false, "Select folder"),
            Self::Executable(_) => unreachable!("Executable paths use the filtered native picker"),
            Self::WorkingDirectory(_) => (false, true, false, "Select working directory"),
        };
        PathPromptOptions {
            files,
            directories,
            multiple,
            prompt: Some(prompt.into()),
        }
    }
}

struct QueuedFile {
    id: Uuid,
    path: PathBuf,
    canonical_path: PathBuf,
    status: FileStatus,
    exit_code: Option<i32>,
    failure_detail: Option<String>,
}

#[derive(Default)]
struct FileSelection {
    selected: HashSet<Uuid>,
    anchor: Option<Uuid>,
    cursor: Option<Uuid>,
}

impl FileSelection {
    fn clear(&mut self) {
        self.selected.clear();
        self.anchor = None;
        self.cursor = None;
    }

    fn select_only(&mut self, id: Uuid) {
        self.selected.clear();
        self.selected.insert(id);
        self.anchor = Some(id);
        self.cursor = Some(id);
    }

    fn select_all(&mut self, files: &[QueuedFile], visible: &[usize]) {
        self.selected = visible
            .iter()
            .filter_map(|&index| files.get(index).map(|file| file.id))
            .collect();
        self.anchor = visible
            .first()
            .and_then(|&index| files.get(index))
            .map(|file| file.id);
        self.cursor = visible
            .last()
            .and_then(|&index| files.get(index))
            .map(|file| file.id);
        if visible.is_empty() {
            self.clear();
        }
    }

    fn move_cursor(
        &mut self,
        files: &[QueuedFile],
        visible: &[usize],
        delta: isize,
        extend: bool,
    ) -> Option<Uuid> {
        if visible.is_empty() {
            return None;
        }
        let index_of = |id: Option<Uuid>| {
            id.and_then(|id| {
                visible
                    .iter()
                    .position(|&file_index| files.get(file_index).is_some_and(|file| file.id == id))
            })
        };
        let current = index_of(self.cursor)
            .or_else(|| index_of(self.anchor))
            .or_else(|| {
                visible.iter().position(|&file_index| {
                    files
                        .get(file_index)
                        .is_some_and(|file| self.selected.contains(&file.id))
                })
            })
            .unwrap_or(0);
        let next = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current
                .saturating_add(delta as usize)
                .min(visible.len().saturating_sub(1))
        };
        let id = files.get(visible[next])?.id;
        if extend {
            let anchor_index = index_of(self.anchor).unwrap_or(next);
            if self.anchor.is_none() {
                self.anchor = Some(id);
            }
            let lo = anchor_index.min(next);
            let hi = anchor_index.max(next);
            self.selected.clear();
            self.selected.extend(
                visible[lo..=hi]
                    .iter()
                    .filter_map(|&file_index| files.get(file_index).map(|file| file.id)),
            );
            self.cursor = Some(id);
        } else {
            self.select_only(id);
        }
        Some(id)
    }

    fn click(
        &mut self,
        files: &[QueuedFile],
        visible: &[usize],
        index: usize,
        ctrl: bool,
        shift: bool,
    ) {
        let Some(file) = files.get(index) else { return };
        let id = file.id;
        if shift {
            if visible.is_empty() {
                return;
            }
            let end = visible.iter().position(|&i| i == index).unwrap_or(0);
            let start = self
                .anchor
                .and_then(|anchor| {
                    visible
                        .iter()
                        .position(|&i| files.get(i).is_some_and(|f| f.id == anchor))
                })
                .unwrap_or(end);
            if self.anchor.is_none() {
                self.anchor = Some(id);
            }
            if !ctrl {
                self.selected.clear();
            }
            for &i in &visible[start.min(end)..=start.max(end)] {
                if let Some(file) = files.get(i) {
                    self.selected.insert(file.id);
                }
            }
            self.cursor = Some(id);
        } else if ctrl {
            if !self.selected.remove(&id) {
                self.selected.insert(id);
                self.anchor = Some(id);
            }
            self.cursor = Some(id);
        } else if self.selected.len() == 1 && self.selected.contains(&id) {
            self.clear();
        } else {
            self.select_only(id);
        }
    }
}

struct TabState {
    profile: CliProfile,
    settings_expanded: bool,
    settings_error: Option<String>,
    name: Entity<InputState>,
    exe: Entity<InputState>,
    args: Entity<TextareaState>,
    directory: Entity<InputState>,
    file_filter: Entity<InputState>,
    file_filter_text: String,
    files: Vec<QueuedFile>,
    selection: FileSelection,
    run_selected_only: bool,
    status_filter: Option<String>,
    draft_non_zero_exit: NonZeroExit,
    draft_log_history: bool,
}

impl TabState {
    fn new(profile: CliProfile, window: &mut Window, cx: &mut Context<BatchRunner>) -> Self {
        let name_value = profile.display_name.clone();
        let exe_value = profile.exe_path.clone();
        let args_value = profile.args_template.clone();
        let directory_value = profile.working_directory.clone();
        let name = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("CLI profile name");
            state.set_value(name_value, window, cx);
            state
        });
        let exe = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("Path to CLI executable");
            state.set_value(exe_value, window, cx);
            state
        });
        let args = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx).placeholder("{input}");
            state.set_value(args_value, window, cx);
            state
        });
        let directory = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("Executable folder (default)");
            state.set_value(directory_value, window, cx);
            state
        });
        let file_filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter files..."));
        let filter_tab_id = profile.id;
        cx.subscribe(&file_filter, move |this, filter, event: &InputEvent, cx| {
            if !matches!(event, InputEvent::Change) || this.active_job.is_some() {
                return;
            }
            let value = filter.read(cx).value().to_string();
            let changed = if let Some(tab) = this
                .tabs
                .iter_mut()
                .find(|tab| tab.profile.id == filter_tab_id)
                && tab.file_filter_text != value
            {
                tab.file_filter_text = value;
                tab.selection.clear();
                true
            } else {
                false
            };
            if changed {
                if this.selected_id() == Some(filter_tab_id) {
                    this.queue_scroll
                        .scroll_to_item(0, gpui_kit::ScrollStrategy::Top);
                }
                cx.notify();
            }
        })
        .detach();
        Self {
            draft_non_zero_exit: profile.on_non_zero_exit,
            draft_log_history: profile.keep_job_log_history,
            profile,
            settings_expanded: false,
            settings_error: None,
            name,
            exe,
            args,
            directory,
            file_filter,
            file_filter_text: String::new(),
            files: Vec::new(),
            selection: FileSelection::default(),
            run_selected_only: false,
            status_filter: None,
        }
    }

    fn add_path(&mut self, path: PathBuf) -> Result<bool, String> {
        let absolute = if path.is_absolute() {
            path.clone()
        } else {
            std::env::current_dir()
                .map_err(|e| format!("Could not resolve current directory: {e}"))?
                .join(&path)
        };
        let canonical = absolute
            .canonicalize()
            .map_err(|e| format!("Could not add {}: {e}", path.display()))?;
        let metadata = fs::metadata(&canonical)
            .map_err(|e| format!("Could not inspect {}: {e}", path.display()))?;
        if !metadata.is_file()
            || self
                .files
                .iter()
                .any(|file| file.canonical_path == canonical)
        {
            return Ok(false);
        }
        let id = Uuid::new_v4();
        self.files.push(QueuedFile {
            id,
            path: absolute,
            canonical_path: canonical,
            status: FileStatus::Pending,
            exit_code: None,
            failure_detail: None,
        });
        self.selection.clear();
        Ok(true)
    }

    fn remove_selected(&mut self) {
        let Some(first) = self
            .files
            .iter()
            .position(|file| self.selection.selected.contains(&file.id))
        else {
            self.selection.clear();
            return;
        };
        self.files
            .retain(|file| !self.selection.selected.contains(&file.id));
        self.status_filter = None;
        if self.files.is_empty() {
            self.selection.clear();
        } else {
            self.selection
                .select_only(self.files[first.min(self.files.len() - 1)].id);
        }
    }

    fn clear(&mut self) {
        self.files.clear();
        self.selection.clear();
        self.status_filter = None;
    }

    fn selected_visible_indices(&self) -> Vec<usize> {
        self.visible_indices()
            .into_iter()
            .filter(|&index| self.selection.selected.contains(&self.files[index].id))
            .collect()
    }

    fn visible_indices(&self) -> Vec<usize> {
        let file_filter = self.file_filter_text.to_lowercase();
        self.files
            .iter()
            .enumerate()
            .filter_map(|(i, file)| {
                let status_matches = self
                    .status_filter
                    .as_ref()
                    .is_none_or(|filter| file_status_text(file) == *filter);
                let file_name_matches = file_filter.is_empty()
                    || display_file_name(&file.path)
                        .to_lowercase()
                        .contains(&file_filter);
                (status_matches && file_name_matches).then_some(i)
            })
            .collect()
    }
}

struct ActiveJob {
    tab_id: Uuid,
    total: usize,
    completed: usize,
    started: Instant,
    current_file: Option<String>,
    stop: Arc<AtomicBool>,
}

struct FileRow {
    file_id: Uuid,
    original_index: usize,
    path: String,
    tooltip_path: String,
    status: String,
    status_type: FileStatus,
    selected: bool,
    log_enabled: bool,
}

pub struct BatchRunner {
    config: AppConfig,
    config_save_allowed: bool,
    tabs: Vec<TabState>,
    selected_tab_id: Option<Uuid>,
    app_focus: FocusHandle,
    queue_focus: FocusHandle,
    queue_scroll: VirtualListScrollHandle,
    tab_scroll: ScrollHandle,
    tab_context_target: Rc<Cell<Option<Uuid>>>,
    status_filter_select: Entity<SelectState<SearchableVec<String>>>,
    status_filter_select_options: Vec<String>,
    status_filter_select_tab: Option<Uuid>,
    status_filter_select_value: String,
    active_job: Option<ActiveJob>,
    receiver: Option<Receiver<RunnerEvent>>,
    runner_task: Option<gpui_kit::Task<()>>,
    logs: HashMap<Uuid, JobLog>,
    histories: HashMap<Uuid, Vec<JobLog>>,
    file_logs: HashMap<(Uuid, Uuid), String>,
    history_job_id: Option<Uuid>,
    exporting: bool,
    log_selection: TextSelectionHandle,
    help_selections: Vec<TextSelectionHandle>,
    help_scroll: ScrollHandle,
    log_copy_text: String,
    log_all_selected: bool,
    help_all_selected: bool,
    banner: Option<String>,
    show_about: bool,
    show_help: bool,
    help_show_examples: bool,
    delete_tab_id: Option<Uuid>,
    rename_tab_id: Option<Uuid>,
    rename_input: Entity<InputState>,
    rename_error: Option<String>,
    log_dialog: Option<(Uuid, Option<Uuid>)>,
    last_window_position: Option<[f32; 2]>,
    last_position_write: Instant,
    cleanup_on_drop: bool,
}

impl BatchRunner {
    pub fn new(
        mut config: AppConfig,
        load_warning: Option<String>,
        config_save_allowed: bool,
        prepare_logs: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        config.normalize();
        let initial_window_position = window.bounds().origin;
        let mut banner = load_warning;
        if prepare_logs {
            match log_store::prepare_session() {
                Ok(warnings) if !warnings.is_empty() => banner = Some(warnings.join(" ")),
                Err(error) => banner = Some(error),
                _ => {}
            }
        }
        let tabs: Vec<TabState> = config
            .tabs
            .iter()
            .cloned()
            .map(|profile| TabState::new(profile, window, cx))
            .collect();
        let select_tab_id = config.selected_tab_id;
        let selected_tab =
            select_tab_id.and_then(|id| tabs.iter().find(|tab| tab.profile.id == id));
        let status_filter_select_options = status_select_options(selected_tab);
        let status_filter_select_value = selected_tab
            .and_then(|tab| tab.status_filter.clone())
            .unwrap_or_else(|| "ALL".to_owned());
        let select_index = status_filter_select_options
            .iter()
            .position(|option| option == &status_filter_select_value)
            .map(IndexPath::new);
        let status_filter_select = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(status_filter_select_options.clone()),
                select_index,
                window,
                cx,
            )
        });
        cx.subscribe(
            &status_filter_select,
            |this, _, event: &SelectEvent<SearchableVec<String>>, cx| {
                let SelectEvent::Confirm(value) = event;
                let Some(tab_id) = this.selected_id() else {
                    return;
                };
                this.set_filter(tab_id, value.clone().filter(|value| value != "ALL"));
                cx.notify();
            },
        )
        .detach();
        let tab_scroll = ScrollHandle::new();
        let initial_tab_index = config
            .selected_tab_id
            .and_then(|selected_id| {
                config
                    .tabs
                    .iter()
                    .position(|profile| profile.id == selected_id)
            })
            .unwrap_or_default();
        tab_scroll.scroll_to_item(initial_tab_index);
        let rename_input = cx.new(|cx| InputState::new(window, cx).placeholder("Tab name"));
        let log_selection = TextSelectionHandle::new("", cx);
        let help_selections = (0..(1 + HELP_EXAMPLES.len() * 3))
            .map(|_| TextSelectionHandle::new("", cx))
            .collect();
        let mut histories = HashMap::new();
        let mut logs = HashMap::new();
        if prepare_logs {
            for profile in &config.tabs {
                match log_store::load_history(profile.id) {
                    Ok(mut history) => {
                        history.truncate(if profile.keep_job_log_history { 20 } else { 1 });
                        for log in &mut history {
                            if log.finished_at.is_none() {
                                log.aborted = true;
                                log.stop_reason = Some(
                                    "Previous app session ended before the job completed.".into(),
                                );
                            }
                        }
                        if let Err(error) = log_store::save_history(profile.id, &history) {
                            banner = Some(error);
                        }
                        if let Some(latest) = history.first() {
                            logs.insert(profile.id, latest.clone());
                        }
                        histories.insert(profile.id, history);
                    }
                    Err(error) => banner = Some(error),
                }
            }
        }
        Self {
            selected_tab_id: config.selected_tab_id,
            config,
            config_save_allowed,
            tabs,
            app_focus: cx.focus_handle(),
            queue_focus: cx.focus_handle(),
            queue_scroll: VirtualListScrollHandle::new(),
            tab_scroll,
            tab_context_target: Rc::new(Cell::new(None)),
            status_filter_select,
            status_filter_select_options,
            status_filter_select_tab: select_tab_id,
            status_filter_select_value,
            active_job: None,
            receiver: None,
            runner_task: None,
            logs,
            histories,
            file_logs: HashMap::new(),
            history_job_id: None,
            exporting: false,
            log_selection,
            help_selections,
            help_scroll: ScrollHandle::default(),
            log_copy_text: String::new(),
            log_all_selected: false,
            help_all_selected: false,
            banner,
            show_about: false,
            show_help: false,
            help_show_examples: false,
            delete_tab_id: None,
            rename_tab_id: None,
            rename_input,
            rename_error: None,
            log_dialog: None,
            last_window_position: Some([
                initial_window_position.x.as_f32(),
                initial_window_position.y.as_f32(),
            ]),
            last_position_write: Instant::now(),
            cleanup_on_drop: prepare_logs,
        }
    }

    fn selected_index(&self) -> Option<usize> {
        self.selected_tab_id
            .and_then(|id| self.tabs.iter().position(|tab| tab.profile.id == id))
    }

    fn selected_id(&self) -> Option<Uuid> {
        self.selected_index()
            .map(|index| self.tabs[index].profile.id)
    }

    fn settings_mode_active(&self) -> bool {
        self.selected_index()
            .is_some_and(|index| self.tabs[index].settings_expanded)
    }

    fn locked(&self, tab_id: Uuid) -> bool {
        self.active_job
            .as_ref()
            .is_some_and(|job| job.tab_id == tab_id)
    }

    fn can_start(&self) -> bool {
        self.active_job.is_none()
            && self.selected_index().is_some_and(|index| {
                let tab = &self.tabs[index];
                !tab.visible_indices().is_empty() && cli::validate_profile(&tab.profile).is_ok()
            })
    }

    fn config_snapshot(&self) -> AppConfig {
        let mut next = self.config.clone();
        next.tabs = self.tabs.iter().map(|tab| tab.profile.clone()).collect();
        next.selected_tab_id = self.selected_tab_id;
        next
    }

    fn save_config(&mut self) {
        self.config = self.config_snapshot();
        if !self.config_save_allowed {
            self.banner = Some(SAVE_DISABLED.to_owned());
        } else if let Err(error) = config::save(&self.config) {
            self.banner = Some(error);
        }
    }

    fn select_theme(&mut self, name: String, cx: &mut Context<Self>) {
        let name = crate::theme::resolve_theme_name(&name, cx);
        if !crate::theme::apply(&name, cx) {
            self.banner = Some(format!("The theme '{name}' is not available."));
            cx.notify();
            return;
        }

        let mut next = self.config_snapshot();
        next.theme = name;
        self.config = next;
        if !self.config_save_allowed {
            self.banner = Some(SAVE_DISABLED.to_owned());
        } else if let Err(error) = config::save(&self.config) {
            self.banner = Some(error);
        }
        cx.notify();
    }

    fn select_tab(&mut self, id: Uuid) {
        if self.active_job.is_some() {
            return;
        }
        if self.tabs.iter().any(|tab| tab.profile.id == id) {
            if self.selected_tab_id != Some(id) {
                self.queue_scroll
                    .scroll_to_item(0, gpui_kit::ScrollStrategy::Top);
            }
            self.selected_tab_id = Some(id);
            self.save_config();
        }
    }

    fn create_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_job.is_some() {
            return;
        }
        let profile = CliProfile::new(self.tabs.len() + 1);
        let id = profile.id;
        self.tabs.push(TabState::new(profile, window, cx));
        self.selected_tab_id = Some(id);
        self.queue_scroll
            .scroll_to_item(0, gpui_kit::ScrollStrategy::Top);
        self.tab_scroll.scroll_to_item(self.tabs.len() - 1);
        self.save_config();
        cx.notify();
    }

    fn duplicate_tab(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_job.is_some() {
            return;
        }
        let Some(index) = self.tabs.iter().position(|tab| tab.profile.id == id) else {
            return;
        };
        let mut profile = self.tabs[index].profile.clone();
        let base = if profile.display_name.trim().is_empty() {
            "CLI"
        } else {
            profile.display_name.trim()
        };
        profile.id = Uuid::new_v4();
        profile.display_name = format!("{base} (Copy)");
        let duplicate_id = profile.id;
        self.tabs
            .insert(index + 1, TabState::new(profile, window, cx));
        self.selected_tab_id = Some(duplicate_id);
        self.queue_scroll
            .scroll_to_item(0, gpui_kit::ScrollStrategy::Top);
        self.tab_scroll.scroll_to_item(index + 1);
        self.save_config();
        cx.notify();
    }

    fn move_tab(&mut self, id: Uuid, delta: isize) {
        if self.active_job.is_some() {
            return;
        }
        let Some(index) = self.tabs.iter().position(|tab| tab.profile.id == id) else {
            return;
        };
        let next = index as isize + delta;
        if next < 0 || next >= self.tabs.len() as isize {
            return;
        }
        self.tabs.swap(index, next as usize);
        self.tab_scroll.scroll_to_item(next as usize);
        self.save_config();
    }

    fn begin_rename(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_job.is_some() {
            return;
        }
        let Some(tab) = self.tabs.iter().find(|tab| tab.profile.id == id) else {
            return;
        };
        let name = tab.profile.display_name.clone();
        self.rename_input
            .update(cx, |input, cx| input.set_value(name, window, cx));
        self.rename_tab_id = Some(id);
        self.rename_error = None;
        cx.notify();
    }

    fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.rename_tab_id else { return };
        let name = self
            .rename_input
            .read(cx)
            .value()
            .to_string()
            .trim()
            .to_owned();
        if name.is_empty() {
            self.rename_error = Some("Tab name cannot be empty.".to_owned());
            cx.notify();
            return;
        }
        if !self.config_save_allowed {
            self.rename_error = Some(SAVE_DISABLED.to_owned());
            cx.notify();
            return;
        }
        let Some(index) = self.tabs.iter().position(|tab| tab.profile.id == id) else {
            self.rename_tab_id = None;
            return;
        };
        let mut next = self.config_snapshot();
        if let Some(profile) = next.tabs.iter_mut().find(|profile| profile.id == id) {
            profile.display_name.clone_from(&name);
        }
        if let Err(error) = config::save(&next) {
            self.banner = Some(error.clone());
            self.rename_error = Some(error);
            cx.notify();
            return;
        }
        self.tabs[index].profile.display_name.clone_from(&name);
        self.tabs[index]
            .name
            .update(cx, |input, cx| input.set_value(name, window, cx));
        self.config = next;
        self.rename_tab_id = None;
        self.rename_error = None;
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.delete_tab_id else { return };
        if self.tabs.len() <= 1 || self.locked(id) {
            return;
        }
        let Some(index) = self.tabs.iter().position(|tab| tab.profile.id == id) else {
            return;
        };
        if let Err(error) = log_store::remove_tab_log(id) {
            self.banner = Some(error);
        }
        self.tabs.remove(index);
        self.logs.remove(&id);
        self.histories.remove(&id);
        self.file_logs.retain(|(tab, _), _| *tab != id);
        if self.selected_tab_id == Some(id) {
            self.selected_tab_id = self
                .tabs
                .get(index.min(self.tabs.len().saturating_sub(1)))
                .map(|tab| tab.profile.id);
            self.queue_scroll
                .scroll_to_item(0, gpui_kit::ScrollStrategy::Top);
            if let Some(selected_index) = self.selected_index() {
                self.tab_scroll.scroll_to_item(selected_index);
            }
        }
        self.delete_tab_id = None;
        self.save_config();
        cx.notify();
    }

    fn add_paths(&mut self, tab_id: Uuid, paths: impl IntoIterator<Item = PathBuf>) {
        if self.active_job.is_some() {
            return;
        }
        let Some(index) = self.tabs.iter().position(|tab| tab.profile.id == tab_id) else {
            return;
        };
        let before = self.tabs[index].files.len();
        for path in paths {
            if let Err(error) = self.tabs[index].add_path(path) {
                self.banner = Some(error);
            }
        }
        if self.tabs[index].files.len() != before {
            self.tabs[index].status_filter = None;
        }
    }

    fn pick_files(&mut self, tab_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_job.is_some() {
            return;
        }
        self.prompt_paths(PathDialogPurpose::AddFiles(tab_id), window, cx);
    }

    fn pick_folder(&mut self, tab_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_job.is_some() {
            return;
        }
        self.prompt_paths(PathDialogPurpose::AddFolder(tab_id), window, cx);
    }

    fn prompt_paths(
        &mut self,
        purpose: PathDialogPurpose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_job.is_some() {
            return;
        }
        let window_handle = window.window_handle();
        if let PathDialogPurpose::Executable(tab_id) = purpose {
            cx.spawn(async move |this, cx| {
                let path = rfd::AsyncFileDialog::new()
                    .set_title("Select CLI executable")
                    .add_filter("Windows executable", &["exe"])
                    .pick_file()
                    .await
                    .map(|file| file.path().to_path_buf());
                if let Some(path) = path {
                    let _ = window_handle.update(cx, |_, window, cx| {
                        let _ = this.update(cx, |this, cx| {
                            this.apply_path_selection(
                                PathDialogPurpose::Executable(tab_id),
                                vec![path],
                                window,
                                cx,
                            );
                            cx.notify();
                        });
                    });
                }
            })
            .detach();
            return;
        }
        let receiver = cx.prompt_for_paths(purpose.options());
        cx.spawn(async move |this, cx| match receiver.await {
            Ok(Ok(Some(paths))) if !paths.is_empty() => {
                let _ = window_handle.update(cx, |_, window, cx| {
                    let _ = this.update(cx, |this, cx| {
                        this.apply_path_selection(purpose, paths, window, cx);
                        cx.notify();
                    });
                });
            }
            Ok(Err(error)) => {
                let _ = this.update(cx, |this, cx| {
                    this.banner = Some(format!("Could not open path selection dialog: {error}"));
                    cx.notify();
                });
            }
            _ => {}
        })
        .detach();
    }

    fn apply_path_selection(
        &mut self,
        purpose: PathDialogPurpose,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match purpose {
            PathDialogPurpose::AddFiles(tab_id) => self.add_paths(tab_id, paths),
            PathDialogPurpose::AddFolder(tab_id) => {
                if let Some(folder) = paths.into_iter().next() {
                    match fs::read_dir(&folder) {
                        Ok(entries) => self.add_paths(
                            tab_id,
                            entries.filter_map(Result::ok).map(|entry| entry.path()),
                        ),
                        Err(error) => {
                            self.banner =
                                Some(format!("Could not read {}: {error}", folder.display()))
                        }
                    }
                }
            }
            PathDialogPurpose::Executable(tab_id) => {
                if let Some(path) = paths.into_iter().next() {
                    if path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
                    {
                        self.set_browsed_profile_path(tab_id, Some(path), true, window, cx);
                    } else if !self.locked(tab_id)
                        && let Some(tab) = self.tabs.iter_mut().find(|tab| tab.profile.id == tab_id)
                    {
                        tab.settings_error =
                            Some("Only .exe files can be selected for the executable path.".into());
                    }
                }
            }
            PathDialogPurpose::WorkingDirectory(tab_id) => {
                self.set_browsed_profile_path(tab_id, paths.into_iter().next(), false, window, cx);
            }
        }
    }

    fn set_browsed_profile_path(
        &mut self,
        tab_id: Uuid,
        path: Option<PathBuf>,
        executable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.locked(tab_id) {
            return;
        }
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.profile.id == tab_id) else {
            return;
        };
        let Some(path) = path else {
            return;
        };
        if let Some(value) = path.to_str() {
            if executable {
                tab.exe.update(cx, |state, cx| {
                    state.set_value(value.to_owned(), window, cx)
                });
            } else {
                tab.directory.update(cx, |state, cx| {
                    state.set_value(value.to_owned(), window, cx)
                });
            }
            tab.settings_error = None;
        } else {
            tab.settings_error = Some(if executable {
                "The executable path cannot be represented as Unicode.".to_owned()
            } else {
                "The working directory path cannot be represented as Unicode.".to_owned()
            });
        }
    }

    fn remove_selected(&mut self, tab_id: Uuid) {
        if self.active_job.is_some() {
            return;
        }
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.profile.id == tab_id) {
            tab.remove_selected();
        }
    }

    fn clear_files(&mut self, tab_id: Uuid) {
        if self.active_job.is_some() {
            return;
        }
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.profile.id == tab_id) {
            tab.clear();
        }
    }

    fn save_settings(&mut self, tab_id: Uuid, cx: &mut Context<Self>) {
        if self.locked(tab_id) {
            return;
        }
        let Some(index) = self.tabs.iter().position(|tab| tab.profile.id == tab_id) else {
            return;
        };
        let tab = &self.tabs[index];
        let profile = CliProfile {
            id: tab.profile.id,
            display_name: tab.name.read(cx).value().to_string().trim().to_owned(),
            exe_path: tab.exe.read(cx).value().to_string().trim().to_owned(),
            args_template: tab.args.read(cx).value().to_string(),
            working_directory: tab.directory.read(cx).value().to_string().trim().to_owned(),
            on_non_zero_exit: tab.draft_non_zero_exit,
            keep_job_log_history: tab.draft_log_history,
        };
        if let Err(errors) = cli::validate_profile(&profile) {
            self.tabs[index].settings_error = Some(errors.join(" "));
            cx.notify();
            return;
        }
        if !self.config_save_allowed {
            self.tabs[index].settings_error = Some(SAVE_DISABLED.to_owned());
            cx.notify();
            return;
        }
        let mut next = self.config_snapshot();
        if let Some(target) = next.tabs.iter_mut().find(|target| target.id == tab_id) {
            *target = profile.clone();
        }
        if let Err(error) = config::save(&next) {
            self.tabs[index].settings_error = Some(error);
            cx.notify();
            return;
        }
        self.tabs[index].profile = profile;
        if let Some(history) = self.histories.get_mut(&tab_id) {
            history.truncate(if self.tabs[index].profile.keep_job_log_history {
                20
            } else {
                1
            });
            if let Err(error) = log_store::save_history(tab_id, history) {
                self.banner = Some(error);
            }
        }
        self.tabs[index].settings_error = None;
        self.tabs[index].settings_expanded = false;
        self.config = next;
        cx.notify();
    }

    fn cancel_settings(&mut self, tab_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.profile.id == tab_id) {
            let profile = tab.profile.clone();
            tab.draft_non_zero_exit = profile.on_non_zero_exit;
            tab.draft_log_history = profile.keep_job_log_history;
            tab.name.update(cx, |state, cx| {
                state.set_value(profile.display_name, window, cx)
            });
            tab.exe.update(cx, |state, cx| {
                state.set_value(profile.exe_path, window, cx)
            });
            tab.args.update(cx, |state, cx| {
                state.set_value(profile.args_template, window, cx)
            });
            tab.directory.update(cx, |state, cx| {
                state.set_value(profile.working_directory, window, cx)
            });
            tab.settings_error = None;
            tab.settings_expanded = false;
        }
        cx.notify();
    }

    fn start_job(&mut self, tab_id: Uuid, target: JobTarget, cx: &mut Context<Self>) {
        if self.active_job.is_some() {
            return;
        }
        let Some(index) = self.tabs.iter().position(|tab| tab.profile.id == tab_id) else {
            return;
        };
        let target_indices = match target {
            JobTarget::AllVisible => self.tabs[index].visible_indices(),
            JobTarget::SelectedVisible => self.tabs[index].selected_visible_indices(),
        };
        if target_indices.is_empty() {
            self.banner = Some(match target {
                JobTarget::AllVisible => "No visible files are available to run.".to_owned(),
                JobTarget::SelectedVisible => "Select at least one visible file to run.".to_owned(),
            });
            return;
        }
        let profile = self.tabs[index].profile.clone();
        if let Err(errors) = cli::validate_profile(&profile) {
            self.banner = Some(errors.join(" "));
            return;
        }
        let paths = match cli::resolved_profile_paths(&profile) {
            Ok(paths) => paths,
            Err(error) => {
                self.banner = Some(error);
                return;
            }
        };
        let files: Vec<RunnerFile> = target_indices
            .into_iter()
            .map(|i| RunnerFile {
                id: self.tabs[index].files[i].id,
                path: self.tabs[index].files[i].path.clone(),
            })
            .collect();
        let entries = files
            .iter()
            .map(|file| runner::pending_log_entry(&profile, &paths, file.id, &file.path))
            .collect();
        let log = JobLog {
            job_id: Uuid::new_v4(),
            tab_id,
            tab_name: profile.display_name.clone(),
            started_at: timestamp_now(),
            finished_at: None,
            duration_ms: None,
            aborted: false,
            stop_reason: None,
            files: entries,
        };
        if let Err(error) = log_store::save(&log_store::latest_path(tab_id), &log) {
            self.banner = Some(error);
            return;
        }
        let history_limit = if profile.keep_job_log_history { 20 } else { 1 };
        let previous_history = self
            .histories
            .get(&tab_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if let Err(error) =
            log_store::save_history_with_latest(tab_id, &log, previous_history, history_limit)
        {
            self.banner = Some(error);
            return;
        }
        let mut history = self.histories.remove(&tab_id).unwrap_or_default();
        history.truncate(history_limit.saturating_sub(1));
        history.insert(0, log.clone());
        self.histories.insert(tab_id, history);
        self.logs.insert(tab_id, log);

        let target_ids: HashSet<_> = files.iter().map(|file| file.id).collect();
        for file in &mut self.tabs[index].files {
            if target_ids.contains(&file.id) {
                file.status = FileStatus::Pending;
                file.exit_code = None;
                file.failure_detail = None;
            }
        }
        self.file_logs.retain(|(logged_tab_id, file_id), _| {
            *logged_tab_id != tab_id || !target_ids.contains(file_id)
        });

        let total = files.len();
        let snapshot = JobSnapshot {
            profile,
            paths,
            files,
        };
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let (tx, rx) = mpsc::channel();
        let _worker = thread::spawn(move || runner::run_job(snapshot, worker_stop, tx));
        self.receiver = Some(rx);
        self.active_job = Some(ActiveJob {
            tab_id,
            total,
            completed: 0,
            started: Instant::now(),
            current_file: None,
            stop,
        });
        self.banner = None;

        let receiver = self.receiver.take().expect("job receiver was just created");
        self.runner_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                let events: Vec<_> = receiver.try_iter().collect();
                let done = events
                    .iter()
                    .any(|event| matches!(event, RunnerEvent::Finished { .. }));
                let update = this.update(cx, |view, cx| {
                    view.process_runner_events(events);
                    cx.notify();
                });
                if update.is_err() || done {
                    break;
                }
            }
        }));
    }

    fn stop_job(&mut self) {
        if let Some(job) = &self.active_job {
            job.stop.store(true, Ordering::Relaxed);
        }
    }

    fn process_runner_events(&mut self, events: Vec<RunnerEvent>) {
        for event in events {
            match event {
                RunnerEvent::FileStarted { entry, .. } => {
                    let Some(job) = &mut self.active_job else {
                        continue;
                    };
                    let tab_id = job.tab_id;
                    let log_index = job.completed + 1;
                    let total = job.total;
                    let file_id = entry.file_id;
                    job.current_file = Some(entry.path.clone());
                    self.file_logs.insert(
                        (tab_id, file_id),
                        format_log_entry(&entry, log_index, total),
                    );
                    self.update_job_file_entry(tab_id, entry);
                    self.apply_queue_file_state(tab_id, file_id, FileStatus::Running, None, None);
                }
                RunnerEvent::FileFinished { entry, .. } => {
                    let Some(job) = &mut self.active_job else {
                        continue;
                    };
                    let tab_id = job.tab_id;
                    job.completed += 1;
                    let log_index = job.completed;
                    let total = job.total;
                    let file_id = entry.file_id;
                    let status = entry.status;
                    let exit_code = entry.exit_code;
                    let failure_detail = entry.error.clone();
                    job.current_file = None;
                    self.file_logs.insert(
                        (tab_id, file_id),
                        format_log_entry(&entry, log_index, total),
                    );
                    self.update_job_file_entry(tab_id, entry);
                    self.apply_queue_file_state(tab_id, file_id, status, exit_code, failure_detail);
                    self.save_job_log(tab_id);
                }
                RunnerEvent::Finished {
                    aborted,
                    finished_at,
                    stop_reason,
                } => {
                    if let Some(job) = self.active_job.take() {
                        let tab_id = job.tab_id;
                        if let Some(log) = self.logs.get_mut(&tab_id) {
                            log.aborted = aborted;
                            log.stop_reason = stop_reason.clone();
                            log.finished_at = Some(finished_at);
                            log.duration_ms = Some(job.started.elapsed().as_millis());
                        }
                        self.save_job_log(tab_id);
                        if aborted {
                            self.banner = stop_reason;
                        }
                    }
                }
            }
        }
    }

    fn update_job_file_entry(&mut self, tab_id: Uuid, entry: FileJobLog) {
        if let Some(log) = self.logs.get_mut(&tab_id)
            && let Some(target) = log
                .files
                .iter_mut()
                .find(|target| target.file_id == entry.file_id)
        {
            *target = entry;
        }
    }

    fn apply_queue_file_state(
        &mut self,
        tab_id: Uuid,
        file_id: Uuid,
        status: FileStatus,
        exit_code: Option<i32>,
        failure_detail: Option<String>,
    ) {
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.profile.id == tab_id)
            && let Some(file) = tab.files.iter_mut().find(|file| file.id == file_id)
        {
            file.status = status;
            file.exit_code = exit_code;
            file.failure_detail = failure_detail;
        }
    }

    fn save_job_log(&mut self, tab_id: Uuid) {
        if let Some(log) = self.logs.get(&tab_id) {
            if let Some(history) = self.histories.get_mut(&tab_id) {
                if let Some(latest) = history.first_mut() {
                    *latest = log.clone();
                }
                if let Err(error) = log_store::save_history(tab_id, history) {
                    self.banner = Some(error);
                }
            }
            if let Err(error) = log_store::save(&log_store::latest_path(tab_id), log) {
                self.banner = Some(error);
            }
        }
    }

    fn set_filter(&mut self, tab_id: Uuid, filter: Option<String>) {
        if self.active_job.is_some() {
            return;
        }
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.profile.id == tab_id)
            && tab.status_filter != filter
        {
            tab.status_filter = filter;
            tab.selection.clear();
            self.queue_scroll
                .scroll_to_item(0, gpui_kit::ScrollStrategy::Top);
        }
    }

    fn modal_open(&self) -> bool {
        self.show_about
            || self.show_help
            || self.delete_tab_id.is_some()
            || self.rename_tab_id.is_some()
            || self.log_dialog.is_some()
    }

    fn clear_dialog_text_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        TextSelection::clear(window, cx);
        self.log_selection.set_local_selection(false, cx);
        for selection in &self.help_selections {
            selection.set_local_selection(false, cx);
        }
        self.log_all_selected = false;
        self.help_all_selected = false;
    }

    fn dismiss_top_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_dialog_text_selection(window, cx);
        if self.log_dialog.take().is_some() {
            return;
        }
        if self.rename_tab_id.take().is_some() {
            self.rename_error = None;
            return;
        }
        if self.delete_tab_id.take().is_some() {
            return;
        }
        if self.show_about {
            self.show_about = false;
            return;
        }
        if self.show_help {
            self.show_help = false;
        }
    }

    fn any_text_input_focused(&self, window: &Window, cx: &gpui_kit::App) -> bool {
        self.tabs.iter().any(|tab| {
            tab.name.read(cx).focus_handle(cx).is_focused(window)
                || tab.exe.read(cx).focus_handle(cx).is_focused(window)
                || tab.args.read(cx).focus_handle(cx).is_focused(window)
                || tab.directory.read(cx).focus_handle(cx).is_focused(window)
        }) || self
            .rename_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    }

    fn handle_shortcut(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.to_ascii_lowercase();
        let settings_tab = self
            .selected_index()
            .filter(|&index| self.tabs[index].settings_expanded)
            .map(|index| self.tabs[index].profile.id);

        if key == "escape" {
            if self.modal_open() {
                self.dismiss_top_dialog(window, cx);
                cx.stop_propagation();
                cx.notify();
            } else if let Some(id) = settings_tab {
                self.cancel_settings(id, window, cx);
                cx.stop_propagation();
            }
            return;
        }

        if self.modal_open() {
            let text_dialog_open = self.show_help || self.log_dialog.is_some();
            if text_dialog_open && event.keystroke.modifiers.control {
                match key.as_str() {
                    "a" => {
                        TextSelection::clear(window, cx);
                        if self.log_dialog.is_some() {
                            self.log_all_selected = true;
                        } else if self.show_help {
                            self.help_all_selected = true;
                        }
                        cx.notify();
                    }
                    "c" => {
                        let text = if self.log_dialog.is_some() && self.log_all_selected {
                            self.log_copy_text.clone()
                        } else if self.show_help && self.help_all_selected {
                            help_dialog_copy_text(self.help_show_examples)
                        } else {
                            TextSelection::selected_text(window, cx)
                        };
                        if !text.is_empty() {
                            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text));
                        }
                    }
                    _ => {}
                }
            }
            cx.stop_propagation();
            return;
        }

        if settings_tab.is_some() {
            return;
        }
        if self.any_text_input_focused(window, cx) {
            return;
        }

        if event.keystroke.modifiers.control {
            match key.as_str() {
                "a" if self.active_job.is_none() => {
                    if let Some(index) = self.selected_index() {
                        let tab = &mut self.tabs[index];
                        let visible = tab.visible_indices();
                        tab.selection.select_all(&tab.files, &visible);
                        cx.notify();
                    }
                }
                "c" => self.copy_selected_paths(cx),
                "v" => {
                    if let Some(id) = self.selected_id() {
                        self.add_paths(id, read_clipboard_files());
                        cx.notify();
                    }
                }
                _ => {}
            }
        } else if key == "up" || key == "arrowup" || key == "down" || key == "arrowdown" {
            if !event.keystroke.modifiers.alt
                && self.active_job.is_none()
                && let Some(index) = self.selected_index()
            {
                let tab = &mut self.tabs[index];
                let visible = tab.visible_indices();
                let delta = if key == "up" || key == "arrowup" {
                    -1
                } else {
                    1
                };
                if let Some(id) = tab.selection.move_cursor(
                    &tab.files,
                    &visible,
                    delta,
                    event.keystroke.modifiers.shift,
                ) && let Some(visible_index) = visible.iter().position(|&file_index| {
                    tab.files.get(file_index).is_some_and(|file| file.id == id)
                }) {
                    self.queue_scroll
                        .scroll_to_item(visible_index, gpui_kit::ScrollStrategy::Nearest);
                    cx.notify();
                }
            }
        } else if key == "delete"
            && self.active_job.is_none()
            && let Some(id) = self.selected_id()
        {
            self.remove_selected(id);
            cx.notify();
        }
    }

    fn copy_selected_paths(&mut self, _cx: &mut Context<Self>) {
        let Some(index) = self.selected_index() else {
            return;
        };
        let tab = &self.tabs[index];
        let text = tab
            .visible_indices()
            .into_iter()
            .filter_map(|file_index| {
                let file = &tab.files[file_index];
                tab.selection
                    .selected
                    .contains(&file.id)
                    .then(|| cli::display_path(&file.path))
            })
            .collect::<Vec<_>>()
            .join("\r\n");
        if text.is_empty() {
            return;
        }
        #[cfg(windows)]
        if let Err(error) = clipboard_win::set_clipboard_string(&text) {
            self.banner = Some(format!(
                "Could not copy selected paths to the clipboard: {error:?}"
            ));
        }
        #[cfg(not(windows))]
        _cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text));
    }

    fn format_file_log(&self, tab_id: Uuid, file_id: Uuid) -> Option<String> {
        if let Some(text) = self.file_logs.get(&(tab_id, file_id)) {
            return Some(text.clone());
        }
        let log = self.logs.get(&tab_id)?;
        let (index, entry) = log
            .files
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.file_id == file_id)?;
        Some(format_log_entry(entry, index + 1, log.files.len()))
    }

    fn has_file_log(&self, tab_id: Uuid, file_id: Uuid) -> bool {
        self.file_logs.contains_key(&(tab_id, file_id))
            || self
                .logs
                .get(&tab_id)
                .is_some_and(|log| log.files.iter().any(|entry| entry.file_id == file_id))
    }

    fn format_job_log(&self, tab_id: Uuid) -> Option<String> {
        let log = if let Some(id) = self.history_job_id {
            self.histories
                .get(&tab_id)?
                .iter()
                .find(|log| log.job_id == id)?
        } else {
            self.logs.get(&tab_id)?
        };
        let mut text = format!(
            "Job Log\nTab: {}\nStarted: {}\nFiles: {}\nJob ID: {}\n\n",
            log.tab_name,
            log.started_at,
            log.files.len(),
            log.job_id
        );
        for (index, entry) in log.files.iter().enumerate() {
            text.push_str(&format_log_entry(entry, index + 1, log.files.len()));
            text.push('\n');
        }
        if let Some(finished) = &log.finished_at {
            text.push_str(&format!("Finished: {finished}\n"));
        }
        if let Some(duration) = log.duration_ms {
            text.push_str(&format!("Duration: {:.1}s\n", duration as f64 / 1000.0));
        }
        if log.aborted {
            text.push_str(log.stop_reason.as_deref().unwrap_or("Job stopped."));
            text.push('\n');
        }
        Some(text)
    }

    fn persist_window_bounds(&mut self, window: &Window) {
        if !self.config_save_allowed
            || self.last_position_write.elapsed() < Duration::from_millis(500)
        {
            return;
        }
        let bounds = window.bounds();
        let next = [bounds.origin.x.as_f32(), bounds.origin.y.as_f32()];
        if self
            .last_window_position
            .is_some_and(|old| old.iter().zip(next).all(|(a, b)| (a - b).abs() < 1.0))
        {
            return;
        }
        self.last_window_position = Some(next);
        self.last_position_write = Instant::now();
        self.config.window_x = Some(next[0]);
        self.config.window_y = Some(next[1]);
        self.config.tabs = self.tabs.iter().map(|tab| tab.profile.clone()).collect();
        self.config.selected_tab_id = self.selected_tab_id;
        if let Err(error) = config::save(&self.config) {
            self.banner = Some(error);
        }
    }
}

impl Drop for BatchRunner {
    fn drop(&mut self) {
        if self.cleanup_on_drop {
            let _ = log_store::cleanup_session();
        }
    }
}

impl Render for BatchRunner {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title(APP_TITLE);
        self.persist_window_bounds(window);
        let settings_mode = self.settings_mode_active();
        let modal_owns_keyboard = self.show_about
            || self.show_help
            || self.delete_tab_id.is_some()
            || self.log_dialog.is_some();
        if (modal_owns_keyboard || (settings_mode && !self.any_text_input_focused(window, cx)))
            && !self.app_focus.is_focused(window)
        {
            self.app_focus.focus(window, cx);
        }
        div()
            .id("app-root")
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .track_focus(&self.app_focus)
            .on_key_down(
                cx.listener(|this, event, window, cx| this.handle_shortcut(event, window, cx)),
            )
            .on_drop::<gpui_kit::ExternalPaths>(cx.listener(
                |this, paths: &gpui_kit::ExternalPaths, _, cx| {
                    if this.settings_mode_active() {
                        return;
                    }
                    if let Some(id) = this.selected_id() {
                        this.add_paths(id, paths.0.iter().cloned());
                        cx.notify();
                    }
                },
            ))
            .children(self.render_root_main_content(window, cx))
            .children(self.render_root_feedback(cx))
    }
}

fn display_file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn format_file_count(count: usize) -> String {
    format!("{count} file{}", if count == 1 { "" } else { "s" })
}

fn file_status_text(file: &QueuedFile) -> String {
    status_text(file.status, file.exit_code)
}

fn status_options(tab: &TabState) -> Vec<String> {
    let mut seen = HashSet::new();
    tab.files
        .iter()
        .filter(|file| !matches!(file.status, FileStatus::Pending | FileStatus::Running))
        .map(file_status_text)
        .filter(|status| seen.insert(status.clone()))
        .collect()
}

fn status_select_options(tab: Option<&TabState>) -> Vec<String> {
    let mut options = vec!["ALL".to_owned()];
    if let Some(tab) = tab {
        options.extend(status_options(tab));
        if let Some(selected) = &tab.status_filter
            && !options.contains(selected)
        {
            options.push(selected.clone());
        }
    }
    options
}

fn help_dialog_copy_text(show_examples: bool) -> String {
    if show_examples {
        let mut text = HELP_EXAMPLES_NOTE.to_owned();
        for example in HELP_EXAMPLES {
            text.push_str(&format!(
                "\n{}\nExecutable: {}\nArguments: {}\n{}",
                example.title, example.executable, example.arguments, example.description,
            ));
        }
        text
    } else {
        "Example input file: C:\\Files\\Sample.txt\n\nPlaceholder — Description — Expands to\n{input} — Full input file path — C:\\Files\\Sample.txt\n{input_dir} — Parent directory — C:\\Files\n{input_name} — File name including extension — Sample.txt\n{input_stem} — File name without extension — Sample\n{input_ext} — Extension without dot — txt\n".to_owned()
    }
}

fn status_text(status: FileStatus, exit_code: Option<i32>) -> String {
    match exit_code {
        Some(0) => "Success (Ret0)".to_owned(),
        Some(code) => format!("Ret{code}"),
        None => match status {
            FileStatus::Pending => "Pending".to_owned(),
            FileStatus::Running => "Running".to_owned(),
            FileStatus::Success => "Success (Ret0)".to_owned(),
            FileStatus::Returned => "Ret?".to_owned(),
            FileStatus::Error => "Error".to_owned(),
            FileStatus::Stopped => "Stopped".to_owned(),
        },
    }
}

fn format_log_entry(entry: &FileJobLog, index: usize, total: usize) -> String {
    let args = entry
        .arguments
        .iter()
        .map(|arg| cli::quote_argument(arg))
        .collect::<Vec<_>>()
        .join(" ");
    let mut text = format!(
        "[{index}/{total}] {}\nInput: {}\nCommand: {}\nArguments: {}\nWorking directory: {}\nStatus: {}\n",
        entry.file_name,
        entry.path,
        entry.command,
        args,
        entry.working_directory,
        status_text(entry.status, entry.exit_code),
    );
    if let Some(started) = &entry.started_at {
        text.push_str(&format!("Started: {started}\n"));
    }
    if let Some(finished) = &entry.finished_at {
        text.push_str(&format!("Finished: {finished}\n"));
    }
    if let Some(duration) = entry.duration_ms {
        text.push_str(&format!("Duration: {:.1}s\n", duration as f64 / 1000.0));
    }
    if let Some(code) = entry.exit_code {
        text.push_str(&format!("Exit code: {code}\n"));
    }
    if !entry.stdout.is_empty() {
        text.push_str(&format!("stdout:\n{}\n", entry.stdout));
    }
    if !entry.stderr.is_empty() {
        text.push_str(&format!("stderr:\n{}\n", entry.stderr));
    }
    if let Some(error) = &entry.error {
        text.push_str(&format!("Error: {error}\n"));
    }
    text
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(windows)]
fn read_clipboard_files() -> Vec<PathBuf> {
    use clipboard_win::{Clipboard, Getter, formats};
    if let Ok(_clipboard) = Clipboard::new_attempts(3) {
        let mut files = Vec::new();
        if formats::FileList.read_clipboard(&mut files).is_ok() && !files.is_empty() {
            return files.into_iter().map(PathBuf::from).collect();
        }
        let mut text = String::new();
        if formats::Unicode.read_clipboard(&mut text).is_ok() {
            return text
                .lines()
                .map(|line| PathBuf::from(line.trim().trim_matches('"')))
                .filter(|path| !path.as_os_str().is_empty())
                .collect();
        }
    }
    Vec::new()
}

#[cfg(not(windows))]
fn read_clipboard_files() -> Vec<PathBuf> {
    Vec::new()
}

fn open_external_url(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "shell32")]
        unsafe extern "system" {
            fn ShellExecuteW(
                window: *mut std::ffi::c_void,
                operation: *const u16,
                file: *const u16,
                parameters: *const u16,
                directory: *const u16,
                show_command: i32,
            ) -> isize;
        }
        let operation: Vec<u16> = "open\0".encode_utf16().collect();
        let target: Vec<u16> = std::ffi::OsStr::new(url)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            )
        };
        if result > 32 {
            Ok(())
        } else {
            Err(format!(
                "Could not open the default browser (Windows ShellExecuteW error {result})."
            ))
        }
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        Err("Opening links is supported on Windows only in this build.".to_owned())
    }
}
