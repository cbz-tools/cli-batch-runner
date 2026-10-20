use super::*;
use gpui_kit::AnyElement;

impl BatchRunner {
    pub(super) fn render_settings(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Some(index) = self.selected_index() else {
            return div()
                .id("settings")
                .w_full()
                .p_3()
                .child("No CLI tab. Use + Tab to add one.")
                .into_any_element();
        };
        let tab = &self.tabs[index];
        let id = tab.profile.id;
        if tab.settings_expanded {
            return self.render_settings_mode(id, cx).into_any_element();
        }
        let name = tab.profile.display_name.clone();
        let exe_path = tab.profile.exe_path.trim();
        let display_exe_path = exe_path
            .strip_prefix('"')
            .and_then(|path| path.strip_suffix('"'))
            .unwrap_or(exe_path);
        let exe_file_name = if display_exe_path.is_empty() {
            "Not configured".to_owned()
        } else {
            Path::new(display_exe_path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(display_exe_path)
                .to_owned()
        };
        let settings_hover_background = cx.theme().primary.opacity(0.06);
        let settings_hover_border = cx.theme().primary.opacity(0.45);
        let panel = div()
            .id("settings")
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .p_3()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_lg()
            .bg(cx.theme().muted)
            .child(
                Button::new("settings-toggle")
                    .outline()
                    .w_full()
                    .justify_start()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_1()
                    .border_color(cx.theme().muted_foreground.opacity(0.45))
                    .rounded_md()
                    .bg(cx.theme().secondary)
                    .hover(move |style| {
                        style
                            .bg(settings_hover_background)
                            .border_color(settings_hover_border)
                    })
                    .icon(Icon::new(LucideIcon::Settings2))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w(px(0.))
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("CLI Settings"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .whitespace_nowrap()
                                    .text_ellipsis_middle()
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .whitespace_nowrap()
                                    .child("EXE"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .whitespace_nowrap()
                                    .text_ellipsis_middle()
                                    .child(exe_file_name),
                            ),
                    )
                    .child(Icon::new(LucideIcon::ChevronRight))
                    .tooltip("Open CLI Settings")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.app_focus.focus(window, cx);
                        if let Some(tab) = this.tabs.iter_mut().find(|tab| tab.profile.id == id) {
                            tab.settings_expanded = true;
                        }
                        cx.notify();
                    })),
            );
        panel.into_any_element()
    }

    fn render_settings_mode(&mut self, id: Uuid, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(index) = self.tabs.iter().position(|tab| tab.profile.id == id) else {
            return div().id("settings-mode").into_any_element();
        };
        let tab = &self.tabs[index];
        let locked = self.locked(id);
        let display_name = if tab.profile.display_name.trim().is_empty() {
            "CLI".to_owned()
        } else {
            tab.profile.display_name.clone()
        };

        div()
            .id("settings-mode")
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.))
            .w_full()
            .gap_3()
            .p_3()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_lg()
            .bg(cx.theme().muted)
            .child(
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w_full()
                    .h(px(32.))
                    .child(
                        div()
                            .w_full()
                            .px_10()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_center()
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .child(format!("CLI Settings — {display_name}")),
                    )
                    .child(
                        Button::new("settings-cancel-header")
                            .icon(Icon::new(LucideIcon::X))
                            .absolute()
                            .right_0()
                            .accessibility_label("Close settings")
                            .tooltip("Close settings")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.cancel_settings(id, window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .id("settings-form-scroll")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.))
                    .gap_3()
                    .overflow_y_scroll()
                    .children(self.render_settings_profile_fields(id, cx))
                    .children(self.render_settings_argument_fields(id, cx))
                    .children(self.render_settings_directory_field(id, cx))
                    .children(self.render_settings_job_options(id, cx)),
            )
            .child(div().h(px(1.)).w_full().bg(cx.theme().border))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("settings-cancel")
                            .tooltip("Discard settings changes")
                            .label("Cancel")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.cancel_settings(id, window, cx)
                            })),
                    )
                    .child(
                        Button::new("settings-save")
                            .tooltip("Validate and save CLI settings")
                            .primary()
                            .icon(Icon::new(LucideIcon::Save))
                            .label("Save")
                            .disabled(locked)
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.save_settings(id, cx)),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_settings_profile_fields(&self, id: Uuid, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(index) = self.tabs.iter().position(|tab| tab.profile.id == id) else {
            return Vec::new();
        };
        let tab = &self.tabs[index];
        let locked = self.locked(id);
        vec![
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(div().w(px(130.)).text_sm().child("Display name"))
                .child(
                    Input::new(&tab.name)
                        .id("profile-name")
                        .aria_label("Display name")
                        .disabled(locked)
                        .flex_1(),
                )
                .into_any_element(),
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(div().w(px(130.)).text_sm().child("Executable path"))
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .gap_2()
                        .child(
                            Input::new(&tab.exe)
                                .id("exe-path")
                                .aria_label("Executable path")
                                .disabled(locked)
                                .flex_1(),
                        )
                        .child(
                            Button::new("browse-exe")
                                .tooltip("Choose a CLI executable")
                                .icon(Icon::new(LucideIcon::FolderOpen))
                                .label("Browse")
                                .disabled(locked)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.prompt_paths(
                                        PathDialogPurpose::Executable(id),
                                        window,
                                        cx,
                                    );
                                })),
                        ),
                )
                .into_any_element(),
        ]
    }

    fn render_settings_argument_fields(&self, id: Uuid, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(index) = self.tabs.iter().position(|tab| tab.profile.id == id) else {
            return Vec::new();
        };
        let tab = &self.tabs[index];
        let locked = self.locked(id);
        vec![
            div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(div().text_sm().child("Arguments"))
                                        .child(
                                            Button::new("argument-help").tooltip("Show argument placeholders and examples")
                                                .icon(Icon::new(LucideIcon::BookOpen))
                                                .label("Help / Examples")
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    for selection in &this.help_selections {
                                                        selection.set_local_selection(false, cx);
                                                    }
                                                    TextSelection::clear(window, cx);
                                                    this.help_all_selected = false;
                                                    this.show_help = true;
                                                    cx.notify();
                                                })),
                                        ).into_any_element(),
            Textarea::new(&tab.args)
                                        .aria_label("Arguments")
                                        .accessibility_id("arguments")
                                        .h(px(126.))
                                        .disabled(locked).into_any_element(),
            div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("Empty arguments default to {input}. Placeholders: {input}, {input_dir}, {input_name}, {input_stem}, {input_ext}.").into_any_element(),
        ]
    }

    fn render_settings_directory_field(&self, id: Uuid, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(index) = self.tabs.iter().position(|tab| tab.profile.id == id) else {
            return Vec::new();
        };
        let tab = &self.tabs[index];
        let locked = self.locked(id);
        vec![
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(div().w(px(130.)).text_sm().child("Working directory"))
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .gap_2()
                        .child(
                            Input::new(&tab.directory)
                                .id("working-directory")
                                .aria_label("Working directory")
                                .disabled(locked)
                                .flex_1(),
                        )
                        .child(
                            Button::new("browse-directory")
                                .tooltip("Choose a working directory")
                                .icon(Icon::new(LucideIcon::FolderOpen))
                                .label("Browse")
                                .disabled(locked)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.prompt_paths(
                                        PathDialogPurpose::WorkingDirectory(id),
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new("clear-working-directory")
                                .tooltip("Use the executable folder as working directory")
                                .icon(Icon::new(LucideIcon::X))
                                .label("Clear")
                                .disabled(locked)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if let Some(tab) =
                                        this.tabs.iter_mut().find(|tab| tab.profile.id == id)
                                    {
                                        tab.directory.update(cx, |state, cx| {
                                            state.set_value(String::new(), window, cx)
                                        });
                                    }
                                    cx.notify();
                                })),
                        ),
                )
                .into_any_element(),
        ]
    }

    fn render_settings_job_options(&self, id: Uuid, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(index) = self.tabs.iter().position(|tab| tab.profile.id == id) else {
            return Vec::new();
        };
        let tab = &self.tabs[index];
        let locked = self.locked(id);
        let mut fields = vec![
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(div().w(px(180.)).text_sm().child("On Non-Zero Exit"))
                .child(
                    Button::new("non-zero-continue")
                        .label("Continue")
                        .disabled(locked)
                        .when(tab.draft_non_zero_exit == NonZeroExit::Continue, |button| {
                            button.primary()
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(tab) = this.tabs.iter_mut().find(|tab| tab.profile.id == id)
                            {
                                tab.draft_non_zero_exit = NonZeroExit::Continue;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("non-zero-stop")
                        .label("Stop")
                        .disabled(locked)
                        .when(tab.draft_non_zero_exit == NonZeroExit::Stop, |button| {
                            button.primary()
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(tab) = this.tabs.iter_mut().find(|tab| tab.profile.id == id)
                            {
                                tab.draft_non_zero_exit = NonZeroExit::Stop;
                            }
                            cx.notify();
                        })),
                )
                .into_any_element(),
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(div().w(px(180.)).text_sm().child("Keep Job Log History"))
                .child(
                    Button::new("keep-log-history")
                        .label(if tab.draft_log_history {
                            "ON (up to 20 runs)"
                        } else {
                            "OFF (latest run only)"
                        })
                        .disabled(locked)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(tab) = this.tabs.iter_mut().find(|tab| tab.profile.id == id)
                            {
                                tab.draft_log_history = !tab.draft_log_history;
                            }
                            cx.notify();
                        })),
                )
                .into_any_element(),
        ];
        if let Some(error) = tab.settings_error.clone() {
            fields.push(
                div()
                    .text_color(cx.theme().danger)
                    .child(error)
                    .into_any_element(),
            );
        }
        fields
    }
}
