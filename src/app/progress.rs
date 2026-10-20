use super::*;
use gpui_kit::AnyElement;

const EXECUTION_BUTTON_WIDTH: f32 = 128.;
const EXECUTION_TOGGLE_BUTTON_WIDTH: f32 = 28.;
const EXECUTION_CONTROL_WIDTH: f32 = EXECUTION_BUTTON_WIDTH + EXECUTION_TOGGLE_BUTTON_WIDTH;
const EXECUTION_BUTTON_HEIGHT: f32 = 32.;

struct ExecutionActionState {
    display_id: Option<Uuid>,
    visible_count: usize,
    selected_count: usize,
    selected_only: bool,
    can_start: bool,
    running: bool,
    stopping: bool,
}
impl BatchRunner {
    pub(super) fn render_progress(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let selected_id = self.selected_id();
        let selected_index = self.selected_index();
        let can_modify = self.active_job.is_none();
        let has_selected_files =
            selected_index.is_some_and(|index| !self.tabs[index].selection.selected.is_empty());
        let total_files = selected_index.map_or(0, |index| self.tabs[index].files.len());
        let running = self.active_job.is_some();
        let display_id = self
            .active_job
            .as_ref()
            .map(|job| job.tab_id)
            .or(selected_id);
        let visible_count =
            selected_index.map_or(0, |index| self.tabs[index].visible_indices().len());
        let selected_count =
            selected_index.map_or(0, |index| self.tabs[index].selected_visible_indices().len());
        let selected_only = selected_index.is_some_and(|index| self.tabs[index].run_selected_only);
        let can_start = self.can_start() && (!selected_only || selected_count > 0);
        let stopping = self
            .active_job
            .as_ref()
            .is_some_and(|job| job.stop.load(Ordering::Relaxed));
        let mut panel = div()
            .id("progress-panel")
            .w_full()
            .p_3()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_lg()
            .flex()
            .flex_col()
            .gap_2()
            .flex_shrink_0()
            .child(self.render_progress_summary(selected_index, cx));
        let file_actions =
            self.render_file_actions(can_modify, selected_id, has_selected_files, total_files, cx);
        let execution_actions = self.render_execution_actions(
            ExecutionActionState {
                display_id,
                visible_count,
                selected_count,
                selected_only,
                can_start,
                running,
                stopping,
            },
            cx,
        );
        panel = panel.child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(file_actions)
                .child(execution_actions),
        );
        panel.into_any_element()
    }

    fn render_progress_summary(
        &self,
        selected_index: Option<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut progress_area = div()
            .id("progress-summary-area")
            .w_full()
            .h(px(36.))
            .min_w_0()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .justify_center()
            .gap_1();
        if let Some(job) = self.active_job.as_ref() {
            let percent = job
                .completed
                .saturating_mul(100)
                .checked_div(job.total)
                .unwrap_or_default();
            let current = job
                .current_file
                .as_deref()
                .and_then(|path| Path::new(path).file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "—".to_owned());
            let fraction = if job.total == 0 {
                0.0
            } else {
                job.completed as f32 / job.total as f32
            };
            progress_area =
                progress_area
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(div().flex_shrink_0().whitespace_nowrap().text_sm().child(
                                format!(
                                    "Completed: {}/{} ({}%) | Elapsed: {} | Current file:",
                                    job.completed,
                                    job.total,
                                    percent,
                                    format_duration(job.started.elapsed())
                                ),
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_sm()
                                    .child(current),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .h(px(8.))
                            .flex_shrink_0()
                            .rounded_full()
                            .bg(cx.theme().muted)
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(fraction))
                                    .rounded_full()
                                    .bg(cx.theme().primary),
                            ),
                    );
        } else if let Some(index) = selected_index {
            let parts = JobResultCounts::from_statuses(
                self.tabs[index]
                    .files
                    .iter()
                    .map(|file| (file.status, file.exit_code)),
            )
            .display_parts("Success (Ret0)", "Error", "Stopped");
            if !parts.is_empty() {
                progress_area = progress_area.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_sm()
                        .child(parts.join(" | ")),
                );
            }
        }
        progress_area.into_any_element()
    }

    fn render_file_actions(
        &self,
        can_modify: bool,
        selected_id: Option<Uuid>,
        has_selected_files: bool,
        total_files: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .flex_shrink_0()
            .child(
                Button::new("file-add")
                    .tooltip("Add files to the queue")
                    .icon(Icon::new(LucideIcon::FilePlus))
                    .label("Add Files")
                    .disabled(!can_modify || selected_id.is_none())
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(id) = this.selected_id() {
                            this.pick_files(id, window, cx);
                        }
                    })),
            )
            .child(
                Button::new("folder-add")
                    .tooltip("Add files from a folder")
                    .icon(Icon::new(LucideIcon::FolderPlus))
                    .label("Add Folder")
                    .disabled(!can_modify || selected_id.is_none())
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(id) = this.selected_id() {
                            this.pick_folder(id, window, cx);
                        }
                    })),
            )
            .child(
                Button::new("remove-files")
                    .tooltip("Remove selected queue items")
                    .icon(Icon::new(LucideIcon::Trash))
                    .label("Remove")
                    .disabled(!can_modify || !has_selected_files)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(id) = this.selected_id() {
                            this.remove_selected(id);
                            cx.notify();
                        }
                    })),
            )
            .child(
                Button::new("clear-files")
                    .tooltip("Clear the queue")
                    .icon(Icon::new(LucideIcon::X))
                    .label("Clear")
                    .disabled(!can_modify || total_files == 0)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(id) = this.selected_id() {
                            this.clear_files(id);
                            cx.notify();
                        }
                    })),
            )
            .into_any_element()
    }

    fn render_execution_actions(
        &self,
        state: ExecutionActionState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ExecutionActionState {
            display_id,
            visible_count,
            selected_count,
            selected_only,
            can_start,
            running,
            stopping,
        } = state;
        let job_log_button = Button::new("job-log")
            .tooltip("View the latest job log")
            .outline()
            .icon(Icon::new(LucideIcon::ScrollText))
            .accessibility_label("Job Log")
            .disabled(display_id.is_none_or(|id| !self.logs.contains_key(&id)))
            .on_click(cx.listener(|this, _, _, cx| {
                let id = this
                    .active_job
                    .as_ref()
                    .map(|job| job.tab_id)
                    .or_else(|| this.selected_id());
                if let Some(id) = id {
                    this.history_job_id = None;
                    this.log_dialog = Some((id, None));
                    cx.notify();
                }
            }));
        let mut actions = div()
            .flex()
            .items_center()
            .gap_2()
            .flex_shrink_0()
            .child(job_log_button);
        let radius = cx.theme().radius;
        let divider = cx.theme().button_primary_foreground.opacity(0.35);
        let action_button = if running {
            Button::new("stop")
                .tooltip("Stop the running batch")
                .danger()
                .icon(Icon::new(LucideIcon::Square))
                .label("Stop")
                .w(px(EXECUTION_BUTTON_WIDTH))
                .h(px(EXECUTION_BUTTON_HEIGHT))
                .accessibility_label("Stop")
                .disabled(stopping)
                .rounded_tl(radius)
                .rounded_tr(px(0.))
                .rounded_bl(radius)
                .rounded_br(px(0.))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.stop_job();
                    cx.notify();
                }))
        } else {
            let count = if selected_only {
                selected_count
            } else {
                visible_count
            };
            let tooltip = if selected_only {
                format!("Run selected {}", format_file_count(count))
            } else {
                format!("Run {}", format_file_count(count))
            };
            let label = if selected_only {
                format!("Selected ({count})")
            } else {
                format!("All ({count})")
            };
            let accessibility_label = if selected_only {
                "Run selected visible files"
            } else {
                "Run all visible files"
            };
            Button::new("start")
                .tooltip(tooltip)
                .primary()
                .icon(Icon::new(LucideIcon::Play))
                .label(label)
                .w(px(EXECUTION_BUTTON_WIDTH))
                .h(px(EXECUTION_BUTTON_HEIGHT))
                .accessibility_label(accessibility_label)
                .disabled(!can_start)
                .rounded_tl(radius)
                .rounded_tr(px(0.))
                .rounded_bl(radius)
                .rounded_br(px(0.))
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(index) = this.selected_index() {
                        let id = this.tabs[index].profile.id;
                        let target = if this.tabs[index].run_selected_only {
                            JobTarget::SelectedVisible
                        } else {
                            JobTarget::AllVisible
                        };
                        this.start_job(id, target, cx);
                    }
                }))
        };

        let toggle_button = Button::new("execution-target-toggle")
            .tooltip(if selected_only {
                "Switch to all visible files"
            } else {
                "Switch to selected visible files"
            })
            .primary()
            .icon(Icon::new(LucideIcon::RefreshCw))
            .w(px(EXECUTION_TOGGLE_BUTTON_WIDTH))
            .h(px(EXECUTION_BUTTON_HEIGHT))
            .accessibility_label(if selected_only {
                "Switch to all visible files"
            } else {
                "Switch to selected visible files"
            })
            .toggled(selected_only)
            .disabled(running)
            .rounded_tl(px(0.))
            .rounded_tr(radius)
            .rounded_bl(px(0.))
            .rounded_br(radius)
            .border_l_1()
            .border_color(divider)
            .on_click(cx.listener(|this, _, _, cx| {
                if this.active_job.is_none()
                    && let Some(index) = this.selected_index()
                {
                    this.tabs[index].run_selected_only = !this.tabs[index].run_selected_only;
                    cx.notify();
                }
            }));
        actions = actions.child(
            div()
                .id("execution-split-button")
                .flex()
                .w(px(EXECUTION_CONTROL_WIDTH))
                .h(px(EXECUTION_BUTTON_HEIGHT))
                .flex_shrink_0()
                .child(action_button)
                .child(toggle_button),
        );
        actions.into_any_element()
    }
}
