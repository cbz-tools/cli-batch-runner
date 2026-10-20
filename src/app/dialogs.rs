use super::*;
use gpui_kit::AnyElement;

impl BatchRunner {
    pub(super) fn render_help_dialog(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut dialog = div()
            .id("help-dialog")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            // GPUI Kit exposes no semantic scrim token; keep a neutral translucent overlay.
            .bg(gpui_kit::hsla(0.0, 0.0, 0.0, 0.35));
        dialog = dialog.occlude();
        let content = if self.help_show_examples {
            self.render_help_examples_content(cx)
        } else {
            self.render_help_placeholders_content(cx)
        };
        dialog.child(
            div()
                .id("help-card")
                .w(relative(0.94))
                .h(relative(0.9))
                .max_w(px(760.))
                .max_h(px(680.))
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap_3()
                .p_4()
                .bg(cx.theme().background)
                .border_1()
                .border_color(cx.theme().border)
                .rounded_lg()
                .overflow_hidden()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(LucideIcon::BookOpen))
                        .child(
                            div()
                                .flex_1()
                                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                .child("Arguments Help"),
                        )
                        .child(
                            Button::new("help-close")
                                .icon(Icon::new(LucideIcon::X))
                                .accessibility_label("Close Help")
                                .tooltip("Close Help")
                                .on_click(cx.listener(|_this, _, window, cx| {
                                    cx.stop_propagation();
                                    cx.defer_in(window, |this, window, cx| {
                                        this.clear_dialog_text_selection(window, cx);
                                        this.show_help = false;
                                        cx.notify();
                                    });
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("help-placeholders").tooltip("Show available placeholders")
                                .icon(Icon::new(LucideIcon::FileText))
                                .when(!self.help_show_examples, |button| button.primary())
                                .label("Placeholders")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.help_all_selected = false;
                                    for selection in &this.help_selections {
                                        selection.set_local_selection(false, cx);
                                    }
                                    TextSelection::clear(window, cx);
                                    this.help_show_examples = false;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("help-examples").tooltip("Show Windows CLI examples")
                                .icon(Icon::new(LucideIcon::ScrollText))
                                .when(self.help_show_examples, |button| button.primary())
                                .label("Examples")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.help_all_selected = false;
                                    for selection in &this.help_selections {
                                        selection.set_local_selection(false, cx);
                                    }
                                    TextSelection::clear(window, cx);
                                    this.help_show_examples = true;
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .id("help-scroll-area")
                        .relative()
                        .w_full()
                        .min_w(px(0.))
                        .flex()
                        .flex_1()
                        .min_h(px(0.))
                        .overflow_hidden()
                        .child(content)
                        .child(
                            Scrollbar::vertical(&self.help_scroll).mode(ScrollbarMode::Always),
                        ),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Arguments are split into individual process arguments without invoking a shell. Single and double quotes group text containing spaces. Unknown placeholders remain unchanged."),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .child(
                            Button::new("help-close-bottom").tooltip("Close Arguments Help")
                                .icon(Icon::new(LucideIcon::X))
                                .label("Close")
                                .on_click(cx.listener(|_this, _, window, cx| {
                                    cx.stop_propagation();
                                    cx.defer_in(window, |this, window, cx| {
                                        this.clear_dialog_text_selection(window, cx);
                                        this.show_help = false;
                                        cx.notify();
                                    });
                                })),
                        ),
                ),
        )
    }

    fn render_help_examples_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut content = div()
            .id("help-content")
            .w_full()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.))
            .min_h(px(0.))
            .gap_3()
            .overflow_y_scroll()
            .track_scroll(&self.help_scroll)
            .pr_3()
            .when(self.help_all_selected, |body| body.bg(cx.theme().selection))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    this.help_all_selected = false;
                    for selection in &this.help_selections {
                        selection.set_local_selection(false, cx);
                    }
                    TextSelection::clear(window, cx);
                    cx.notify();
                }),
            );
        content = content.child(
            SelectableText::with_handle(
                "help-examples-intro",
                self.help_selections[0].clone(),
                HELP_EXAMPLES_NOTE,
            )
            .document_order(0),
        );
        for (index, example) in HELP_EXAMPLES.iter().enumerate() {
            let order = index * 3 + 1;
            content = content.child(
                div()
                    .w_full()
                    .min_w(px(0.))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded_md()
                    .bg(cx.theme().muted)
                    .p_3()
                    .child(
                        div().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(
                            SelectableText::with_handle(
                                format!("help-{}-title", example.id),
                                self.help_selections[order].clone(),
                                format!("{}\n", example.title),
                            )
                            .document_order(order as u64),
                        ),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div().flex_1().min_w(px(0.)).text_sm().child(
                                    SelectableText::with_handle(
                                        format!("help-{}-template", example.id),
                                        self.help_selections[order + 1].clone(),
                                        format!(
                                            "Executable: {}\nArguments: {}\n",
                                            example.executable, example.arguments
                                        ),
                                    )
                                    .document_order(order as u64 + 1),
                                ),
                            )
                            .child(
                                Button::new(format!("copy-{}-template", example.id))
                                    .outline()
                                    .icon(Icon::new(LucideIcon::Copy))
                                    .label("Copy")
                                    .tooltip("Copy argument template")
                                    .flex_shrink_0()
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
                                            example.arguments.to_owned(),
                                        ));
                                    }),
                            ),
                    )
                    .child(
                        div().w_full().min_w(px(0.)).text_sm().child(
                            SelectableText::with_handle(
                                format!("help-{}-note", example.id),
                                self.help_selections[order + 2].clone(),
                                example.description,
                            )
                            .document_order(order as u64 + 2),
                        ),
                    ),
            );
        }
        content.into_any_element()
    }

    fn render_help_placeholders_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let placeholders = [
            ("{input}", "Full input file path", r"C:\Files\Sample.txt"),
            ("{input_dir}", "Parent directory", r"C:\Files"),
            (
                "{input_name}",
                "File name including extension",
                "Sample.txt",
            ),
            ("{input_stem}", "File name without extension", "Sample"),
            ("{input_ext}", "Extension without dot", "txt"),
        ];
        let mut content = div()
            .id("help-content")
            .w_full()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.))
            .min_h(px(0.))
            .gap_3()
            .overflow_y_scroll()
            .track_scroll(&self.help_scroll)
            .pr_3()
            .when(self.help_all_selected, |body| body.bg(cx.theme().selection))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    this.help_all_selected = false;
                    for selection in &this.help_selections {
                        selection.set_local_selection(false, cx);
                    }
                    TextSelection::clear(window, cx);
                    cx.notify();
                }),
            );
        content = content.child(div().text_sm().child(SelectableText::with_handle(
                "help-placeholder-intro",
                self.help_selections[0].clone(),
                "Example input file: C:\\Files\\Sample.txt\n\nPlaceholder — Description — Expands to\n",
            ).document_order(0)));
        for (index, (placeholder, description, expansion)) in placeholders.into_iter().enumerate() {
            let copy_value = placeholder.to_owned();
            let row_text = format!("{placeholder} — {description} — {expansion}\n");
            content = content.child(
                div()
                    .w_full()
                    .min_w(px(0.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .p_2()
                    .child(
                        div().flex_1().text_sm().child(
                            SelectableText::with_handle(
                                format!("help-placeholder-{index}"),
                                self.help_selections[index + 1].clone(),
                                row_text,
                            )
                            .document_order(index as u64 + 1),
                        ),
                    )
                    .child(
                        Button::new(format!("copy-{placeholder}"))
                            .tooltip("Copy placeholder")
                            .outline()
                            .flex_shrink_0()
                            .icon(Icon::new(LucideIcon::Copy))
                            .label("Copy")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
                                    copy_value.clone(),
                                ));
                            }),
                    ),
            );
        }
        content.into_any_element()
    }

    pub(super) fn render_about_dialog(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().downgrade();
        let mut dialog = div()
            .id("about-dialog")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui_kit::hsla(0.0, 0.0, 0.0, 0.35));
        dialog = dialog.occlude();
        dialog.child(
            div()
                .id("about-card")
                .w(px(360.))
                .flex()
                .flex_col()
                .gap_3()
                .p_5()
                .bg(cx.theme().background)
                .border_1()
                .border_color(cx.theme().border)
                .rounded_lg()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child("CLI Batch Runner")
                        .child(
                            Button::new("about-close")
                                .icon(Icon::new(LucideIcon::X))
                                .accessibility_label("Close")
                                .tooltip("Close")
                                .on_click(cx.listener(|_this, _, window, cx| {
                                    cx.stop_propagation();
                                    cx.defer_in(window, |this, _, cx| {
                                        this.show_about = false;
                                        cx.notify();
                                    });
                                })),
                        ),
                )
                .child(format!("Version {}", env!("CARGO_PKG_VERSION")))
                .child(
                    Button::new("about-github")
                        .tooltip("Open the project on GitHub")
                        .outline()
                        .label("GitHub")
                        .on_click({
                            let entity = entity.clone();
                            move |_, _, app| {
                                if let Err(error) = open_external_url(
                                    "https://github.com/cbz-tools/cli-batch-runner",
                                ) {
                                    let _ = entity.update(app, |this, cx| {
                                        this.banner = Some(error);
                                        cx.notify();
                                    });
                                }
                            }
                        }),
                )
                .child(
                    Button::new("about-latest")
                        .tooltip("Open the latest release on GitHub")
                        .outline()
                        .label("Check for latest version")
                        .on_click({
                            move |_, _, app| {
                                if let Err(error) = open_external_url(
                                    "https://github.com/cbz-tools/cli-batch-runner/releases/latest",
                                ) {
                                    let _ = entity.update(app, |this, cx| {
                                        this.banner = Some(error);
                                        cx.notify();
                                    });
                                }
                            }
                        }),
                ),
        )
    }

    pub(super) fn render_delete_dialog(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let id = self.delete_tab_id.unwrap_or_default();
        let name = self
            .tabs
            .iter()
            .find(|tab| tab.profile.id == id)
            .map(|tab| tab.profile.display_name.clone())
            .unwrap_or_else(|| "CLI".to_owned());
        let mut dialog = div()
            .id("delete-dialog")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui_kit::hsla(0.0, 0.0, 0.0, 0.35));
        dialog = dialog.occlude();
        dialog.child(div().id("delete-card").w(px(440.)).flex().flex_col().gap_3().p_5()
                .bg(cx.theme().background).border_1().border_color(cx.theme().border).rounded_lg()
                .child("Delete tab")
                .child(format!("Delete tab \"{name}\"? This removes the tab and its saved configuration. Your files will not be deleted."))
                .child(div().flex().justify_end().gap_2()
                    .child(Button::new("delete-cancel").tooltip("Keep this CLI tab").label("Cancel").on_click(cx.listener(|_this, _, window, cx| {
                        cx.stop_propagation();
                        cx.defer_in(window, |this, _, cx| {
                            this.delete_tab_id = None;
                            cx.notify();
                        });
                    })))
                    .child(Button::new("delete-confirm").tooltip("Delete this CLI tab and its settings").danger().label("Delete").on_click(cx.listener(|_this, _, window, cx| {
                        cx.stop_propagation();
                        cx.defer_in(window, |this, _, cx| this.confirm_delete(cx));
                    })))))
    }

    pub(super) fn render_rename_dialog(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut dialog = div()
            .id("rename-dialog")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui_kit::hsla(0.0, 0.0, 0.0, 0.35));
        dialog = dialog.occlude();
        dialog.child(
            div()
                .id("rename-card")
                .w(px(400.))
                .flex()
                .flex_col()
                .gap_3()
                .p_5()
                .bg(cx.theme().background)
                .border_1()
                .border_color(cx.theme().border)
                .rounded_lg()
                .child("Rename tab")
                .child(
                    Input::new(&self.rename_input)
                        .id("rename-input")
                        .aria_label("Tab name"),
                )
                .child(if let Some(error) = self.rename_error.clone() {
                    div()
                        .text_color(cx.theme().danger)
                        .child(error)
                        .into_any_element()
                } else {
                    div()
                        .child("Enter a non-empty tab name.")
                        .into_any_element()
                })
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("rename-cancel")
                                .tooltip("Discard the tab name change")
                                .label("Cancel")
                                .on_click(cx.listener(|_this, _, window, cx| {
                                    cx.stop_propagation();
                                    cx.defer_in(window, |this, _, cx| {
                                        this.rename_tab_id = None;
                                        this.rename_error = None;
                                        cx.notify();
                                    });
                                })),
                        )
                        .child(
                            Button::new("rename-save")
                                .tooltip("Save the tab name")
                                .primary()
                                .label("Save")
                                .on_click(cx.listener(|_this, _, window, cx| {
                                    cx.stop_propagation();
                                    cx.defer_in(window, |this, window, cx| {
                                        this.commit_rename(window, cx);
                                    });
                                })),
                        ),
                ),
        )
    }

    fn export_log(&mut self, text: String, cx: &mut Context<Self>) {
        if self.exporting {
            return;
        }
        self.exporting = true;
        cx.spawn(async move |this, cx| {
            let file = rfd::AsyncFileDialog::new()
                .set_title("Export displayed log")
                .add_filter("Text file", &["txt"])
                .set_file_name("cli-batch-runner-log.txt")
                .save_file()
                .await;
            let result = if let Some(file) = file {
                let path = file.path().to_path_buf();
                Some(
                    cx.background_executor()
                        .spawn(async move {
                            fs::write(&path, text.as_bytes()).map_err(|error| {
                                format!("Could not export {}: {error}", path.display())
                            })
                        })
                        .await,
                )
            } else {
                None
            };
            let _ = this.update(cx, |this, cx| {
                this.exporting = false;
                if let Some(Err(error)) = result {
                    this.banner = Some(error);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn render_log_history_button(
        &self,
        tab_id: Uuid,
        file_id: Option<Uuid>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let history = self
            .histories
            .get(&tab_id)
            .map(|history| {
                history
                    .iter()
                    .map(|log| (log.job_id, log.started_at.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let history_enabled = file_id.is_none()
            && self
                .tabs
                .iter()
                .any(|tab| tab.profile.id == tab_id && tab.profile.keep_job_log_history);
        let selected_job = self.history_job_id;
        let history_entity = cx.entity().downgrade();
        let history_label = selected_job
            .and_then(|id| history.iter().find(|(job_id, _)| *job_id == id))
            .map(|(job_id, started_at)| format!("{} · {}", started_at, job_id.as_simple()))
            .unwrap_or_else(|| "Latest run".to_owned());
        Button::new("log-history")
            .label(history_label)
            .disabled(!history_enabled)
            .dropdown_menu(move |menu, _, _| {
                let latest_entity = history_entity.clone();
                let mut menu = menu.item(
                    PopupMenuItem::new("Latest run")
                        .checked(selected_job.is_none())
                        .on_click(move |_, window, cx| {
                            let _ = latest_entity.update(cx, |this, cx| {
                                this.history_job_id = None;
                                this.clear_dialog_text_selection(window, cx);
                                cx.notify();
                            });
                        }),
                );
                for (id, started_at) in history.iter().skip(1) {
                    let id = *id;
                    let entity = history_entity.clone();
                    menu = menu.item(
                        PopupMenuItem::new(format!("{} · {}", started_at, id.as_simple()))
                            .checked(selected_job == Some(id))
                            .on_click(move |_, window, cx| {
                                let _ = entity.update(cx, |this, cx| {
                                    this.history_job_id = Some(id);
                                    this.clear_dialog_text_selection(window, cx);
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    pub(super) fn render_log_dialog(
        &mut self,
        tab_id: Uuid,
        file_id: Option<Uuid>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let text = match file_id {
            Some(file_id) => self.format_file_log(tab_id, file_id),
            None => self.format_job_log(tab_id),
        }
        .unwrap_or_else(|| "No log is available.".to_owned());
        let title = if file_id.is_some() {
            "File Log"
        } else {
            "Job Log"
        };
        self.log_copy_text.clone_from(&text);
        let copy = text.clone();
        let export = text.clone();
        let history_button = self.render_log_history_button(tab_id, file_id, cx);
        let mut dialog = div()
            .id("log-dialog")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui_kit::hsla(0.0, 0.0, 0.0, 0.35));
        dialog = dialog.occlude();
        dialog.child(
            div()
                .id("log-card")
                .w(relative(0.94))
                .h(relative(0.9))
                .max_w(px(900.))
                .max_h(px(760.))
                .flex()
                .flex_col()
                .gap_3()
                .p_4()
                .bg(cx.theme().background)
                .border_1()
                .border_color(cx.theme().border)
                .rounded_lg()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(Icon::new(LucideIcon::FileText))
                                .child(title),
                        )
                        .child(
                            Button::new("log-close")
                                .icon(Icon::new(LucideIcon::X))
                                .accessibility_label("Close log")
                                .tooltip("Close log")
                                .on_click(cx.listener(|_this, _, window, cx| {
                                    cx.stop_propagation();
                                    cx.defer_in(window, |this, window, cx| {
                                        this.clear_dialog_text_selection(window, cx);
                                        this.log_dialog = None;
                                        cx.notify();
                                    });
                                })),
                        ),
                )
                .when(file_id.is_none(), |card| card.child(history_button))
                .child(
                    div()
                        .id("log-text")
                        .flex_1()
                        .min_h(px(0.))
                        .w_full()
                        .overflow_scroll()
                        .p_3()
                        .bg(cx.theme().background)
                        .border_1()
                        .border_color(cx.theme().border)
                        .when(self.log_all_selected, |body| body.bg(cx.theme().selection))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _: &MouseDownEvent, window, cx| {
                                this.log_all_selected = false;
                                this.log_selection.set_local_selection(false, cx);
                                TextSelection::clear(window, cx);
                                cx.notify();
                            }),
                        )
                        .child(
                            div().text_sm().child(
                                SelectableText::with_handle(
                                    "log-readable-text",
                                    self.log_selection.clone(),
                                    text,
                                )
                                .document_order(0),
                            ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("log-export")
                                .label("Export")
                                .disabled(self.exporting)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.export_log(export.clone(), cx);
                                })),
                        )
                        .child(
                            Button::new("log-copy")
                                .tooltip("Copy the entire log")
                                .outline()
                                .icon(Icon::new(LucideIcon::Copy))
                                .label("Copy All")
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
                                        copy.clone(),
                                    ));
                                }),
                        )
                        .child(
                            Button::new("log-close-bottom")
                                .tooltip("Close log")
                                .icon(Icon::new(LucideIcon::X))
                                .label("Close")
                                .on_click(cx.listener(|_this, _, window, cx| {
                                    cx.stop_propagation();
                                    cx.defer_in(window, |this, window, cx| {
                                        this.clear_dialog_text_selection(window, cx);
                                        this.log_dialog = None;
                                        cx.notify();
                                    });
                                })),
                        ),
                ),
        )
    }
}
