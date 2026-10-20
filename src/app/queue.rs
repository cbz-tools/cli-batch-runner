use super::*;
use gpui_kit::AnyElement;

const STATUS_FILTER_MENU_WIDTH: f32 = 160.;

impl BatchRunner {
    pub(super) fn render_queue(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected_index = self.selected_index();
        let selected_id = selected_index.map(|index| self.tabs[index].profile.id);
        let total = selected_index.map_or(0, |index| self.tabs[index].files.len());
        let status_filter_field =
            self.render_status_filter(selected_index, selected_id, window, cx);
        let file_filter_field = self.render_file_filter(selected_index, cx);
        let rows = self.build_visible_rows(selected_index);
        let shown = rows.len();
        let list_tab_id = selected_id.unwrap_or_default();
        let list = self.render_file_list(rows, list_tab_id, window, cx);

        let mut queue = div()
            .id("queue")
            .flex()
            .flex_col()
            .gap_2()
            .flex_1()
            .min_h(px(120.));
        if let Some(header) =
            self.render_queue_header(total, file_filter_field, status_filter_field, cx)
        {
            queue = queue.child(header);
        }
        queue.child(self.render_queue_viewport(list, shown, total, list_tab_id, cx))
    }

    fn render_status_filter(
        &mut self,
        selected_index: Option<usize>,
        selected_id: Option<Uuid>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let filter = selected_index.and_then(|index| self.tabs[index].status_filter.clone());
        let select_options = status_select_options(selected_index.map(|i| &self.tabs[i]));
        let select_value = filter.clone().unwrap_or_else(|| "ALL".to_owned());
        if self.status_filter_select_options != select_options
            || self.status_filter_select_tab != selected_id
            || self.status_filter_select_value != select_value
        {
            self.status_filter_select.update(cx, |state, cx| {
                state.set_items(SearchableVec::new(select_options.clone()), window, cx);
                state.set_selected_value(&select_value, window, cx);
            });
            self.status_filter_select_options = select_options;
            self.status_filter_select_tab = selected_id;
            self.status_filter_select_value = select_value;
        }

        let background = if filter.is_some() {
            cx.theme().primary.opacity(0.12)
        } else {
            cx.theme().background
        };
        let border = if filter.is_some() {
            cx.theme().primary.opacity(0.55)
        } else {
            cx.theme().input
        };
        let tooltip = format!(
            "Filter displayed files by status{}",
            filter
                .as_ref()
                .map(|status| format!(": {status}"))
                .unwrap_or_default()
        );
        div()
            .id("status-filter-field")
            .w_full()
            .h(px(28.))
            .flex()
            .items_center()
            .rounded_sm()
            .border_1()
            .border_color(border)
            .bg(background)
            .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
            .child(
                Select::new(&self.status_filter_select)
                    .id("status-filter")
                    .w_full()
                    .h(px(28.))
                    .menu_width(px(STATUS_FILTER_MENU_WIDTH))
                    .accessibility_label("Filter by status")
                    .disabled(self.active_job.is_some()),
            )
            .into_any_element()
    }

    fn render_file_filter(
        &self,
        selected_index: Option<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(index) = selected_index else {
            return div().flex_1().min_w(px(0.)).into_any_element();
        };
        let tab = &self.tabs[index];
        let background = if tab.file_filter_text.is_empty() {
            cx.theme().background
        } else {
            cx.theme().primary.opacity(0.12)
        };
        let border = if tab.file_filter_text.is_empty() {
            cx.theme().input
        } else {
            cx.theme().primary.opacity(0.55)
        };
        div()
            .id("file-filter-field")
            .flex()
            .flex_1()
            .min_w(px(0.))
            .h(px(28.))
            .items_center()
            .px_2()
            .rounded_sm()
            .border_1()
            .border_color(border)
            .bg(background)
            .child(
                Input::new(&tab.file_filter)
                    .id("file-filter")
                    .aria_label("Filter files")
                    .appearance(false)
                    .focus_bordered(false)
                    .disabled(self.active_job.is_some())
                    .flex_1()
                    .min_w(px(0.)),
            )
            .into_any_element()
    }

    fn build_visible_rows(&self, selected_index: Option<usize>) -> Vec<FileRow> {
        let Some(index) = selected_index else {
            return Vec::new();
        };
        let tab = &self.tabs[index];
        let tab_id = tab.profile.id;
        tab.visible_indices()
            .into_iter()
            .map(|original_index| {
                let file = &tab.files[original_index];
                FileRow {
                    file_id: file.id,
                    original_index,
                    path: display_file_name(&file.path),
                    tooltip_path: cli::display_path(&file.path),
                    status: file_status_text(file),
                    status_type: file.status,
                    selected: tab.selection.selected.contains(&file.id),
                    log_enabled: self.has_file_log(tab_id, file.id),
                }
            })
            .collect()
    }

    fn render_file_list(
        &self,
        rows: Vec<FileRow>,
        list_tab_id: Uuid,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shown = rows.len();
        let row_width = (window.bounds().size.width - px(48.)).max(px(360.));
        let row_sizes = Rc::new(vec![size(row_width, px(36.)); shown]);
        let row_models = Rc::new(rows);
        v_virtual_list(
            cx.entity(),
            "file-rows",
            row_sizes,
            move |_, range, _, cx| {
                range
                    .map(|visible_index| {
                        render_file_row(&row_models[visible_index], list_tab_id, cx)
                    })
                    .collect::<Vec<_>>()
            },
        )
        .track_scroll(&self.queue_scroll)
        .into_any_element()
    }

    fn render_queue_header(
        &self,
        total: usize,
        file_filter_field: AnyElement,
        status_filter_field: AnyElement,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if total == 0 {
            return None;
        }
        Some(
            div()
                .id("queue-header")
                .w_full()
                .flex()
                .flex_col()
                .gap_1()
                .py_1()
                .pl_2()
                .pr_0()
                .bg(cx.theme().muted)
                .child(
                    div()
                        .w_full()
                        .h(px(20.))
                        .flex()
                        .items_center()
                        .child(div().flex_1().min_w(px(0.)).mr_3().child("File"))
                        .child(
                            div()
                                .w(px(FILE_STATUS_COLUMN_WIDTH))
                                .flex_shrink_0()
                                .mr_3()
                                .child("Status"),
                        )
                        .child(
                            div()
                                .w(px(FILE_LOG_COLUMN_WIDTH))
                                .flex_shrink_0()
                                .mr_2()
                                .child("Log"),
                        )
                        .child(div().w(px(QUEUE_SCROLLBAR_GUTTER_WIDTH)).flex_shrink_0()),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .child(div().flex_1().min_w(px(0.)).mr_3().child(file_filter_field))
                        .child(
                            div()
                                .w(px(FILE_STATUS_COLUMN_WIDTH))
                                .flex_shrink_0()
                                .mr_3()
                                .child(status_filter_field),
                        )
                        .child(div().w(px(FILE_LOG_COLUMN_WIDTH)).flex_shrink_0().mr_2())
                        .child(div().w(px(QUEUE_SCROLLBAR_GUTTER_WIDTH)).flex_shrink_0()),
                )
                .into_any_element(),
        )
    }

    fn render_queue_viewport(
        &self,
        list: AnyElement,
        shown: usize,
        total: usize,
        list_tab_id: Uuid,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("queue-viewport")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    if this.active_job.is_none() {
                        if let Some(tab) = this
                            .tabs
                            .iter_mut()
                            .find(|tab| tab.profile.id == list_tab_id)
                        {
                            tab.selection.clear();
                        }
                        this.queue_focus.focus(window, cx);
                        cx.notify();
                    }
                }),
            )
            .track_focus(&self.queue_focus)
            .flex_1()
            .w_full()
            .flex()
            .overflow_hidden()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_md()
            .child(
                div()
                    .id("queue-list-viewport")
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .overflow_hidden()
                    .child(if shown == 0 {
                        div()
                            .size_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .child(if total == 0 {
                                "No files added yet."
                            } else {
                                "No files match the current filters."
                            })
                            .child("Drop files here to add them to this tab.")
                            .into_any_element()
                    } else {
                        list
                    }),
            )
            .child(
                div()
                    .id("queue-scrollbar-gutter")
                    .relative()
                    .w(px(QUEUE_SCROLLBAR_GUTTER_WIDTH))
                    .flex_shrink_0()
                    .h_full()
                    .when(shown > 0, |gutter| {
                        gutter.child(
                            Scrollbar::vertical(&self.queue_scroll)
                                .mode(ScrollbarMode::Always)
                                .viewport_from_layout(),
                        )
                    }),
            )
            .into_any_element()
    }
}

fn render_file_row(row: &FileRow, list_tab_id: Uuid, cx: &mut Context<BatchRunner>) -> AnyElement {
    let file_id = row.file_id;
    let original_index = row.original_index;
    let path = row.path.clone();
    let tooltip_path = row.tooltip_path.clone();
    let status = row.status.clone();
    let status_color = match row.status_type {
        FileStatus::Success => Tag::success(),
        FileStatus::Returned | FileStatus::Error => Tag::danger(),
        FileStatus::Pending => Tag::secondary(),
        FileStatus::Running => Tag::info(),
        FileStatus::Stopped => Tag::warning(),
    };
    let status_tag = status_color
        .outline()
        .small()
        .text_size(rems(0.8125))
        .px_2()
        .child(status);
    let log_enabled = row.log_enabled;
    let row_bg = if row.selected {
        cx.theme().primary.opacity(0.13)
    } else {
        cx.theme().background
    };
    let row_foreground = cx.theme().foreground;
    let row_hover = if row.selected {
        cx.theme().primary.opacity(0.24)
    } else {
        cx.theme().list_hover
    };
    div()
        .id(format!("file-row-{file_id}"))
        .w_full()
        .h(px(36.))
        .flex()
        .items_center()
        .gap_3()
        .px_2()
        .bg(row_bg)
        .text_color(row_foreground)
        .hover(move |style| style.bg(row_hover))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                if this.active_job.is_some() {
                    return;
                }
                if let Some(tab) = this
                    .tabs
                    .iter_mut()
                    .find(|tab| tab.profile.id == list_tab_id)
                {
                    let visible = tab.visible_indices();
                    tab.selection.click(
                        &tab.files,
                        &visible,
                        original_index,
                        event.modifiers.control,
                        event.modifiers.shift,
                    );
                }
                this.queue_focus.focus(window, cx);
                cx.notify();
            }),
        )
        .child(
            div()
                .id(format!("file-name-{file_id}"))
                .flex_1()
                .overflow_hidden()
                .child(path)
                .tooltip(move |window, cx| Tooltip::new(tooltip_path.clone()).build(window, cx)),
        )
        .child(
            div()
                .w(px(FILE_STATUS_COLUMN_WIDTH))
                .flex_shrink_0()
                .flex()
                .justify_start()
                .child(status_tag),
        )
        .child(
            div()
                .w(px(FILE_LOG_COLUMN_WIDTH))
                .flex_shrink_0()
                .flex()
                .justify_center()
                .child(
                    Button::new(format!("file-log-{file_id}"))
                        .w(px(32.))
                        .h(px(32.))
                        .accessibility_label("View File Log")
                        .tooltip("View File Log")
                        .icon(Icon::new(LucideIcon::FileText))
                        .outline()
                        .disabled(!log_enabled)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.log_dialog = Some((list_tab_id, Some(file_id)));
                            cx.notify();
                        })),
                ),
        )
        .into_any_element()
}
