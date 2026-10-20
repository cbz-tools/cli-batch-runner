use super::*;

impl BatchRunner {
    pub(super) fn render_header(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.selected_id();
        let selected_index = self.selected_index();
        let selected_locked = selected.is_some_and(|id| self.locked(id));
        let tab_count = self.tabs.len();
        let entity = cx.entity().downgrade();
        let theme_names = crate::theme::theme_names(cx);
        let selected_theme = self.config.theme.clone();
        let tab_strip = self.render_tab_strip(selected_index, tab_count, cx);
        div().id("header").flex().flex_col().w_full().child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .gap_1()
                        .child(tab_strip)
                        .child(
                            Button::new("add-tab")
                                .disabled(self.active_job.is_some())
                                .icon(Icon::new(LucideIcon::Plus))
                                .accessibility_label("Add tab")
                                .tooltip("Add tab")
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.create_tab(window, cx)),
                                ),
                        )
                        .child(
                            Button::new("delete-tab")
                                .icon(Icon::new(LucideIcon::Trash))
                                .accessibility_label("Delete tab")
                                .tooltip("Delete tab")
                                .disabled(tab_count <= 1 || selected_locked)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(id) = this.selected_id() {
                                        this.delete_tab_id = Some(id);
                                        cx.notify();
                                    }
                                })),
                        )
                        .child(
                            Button::new("move-left")
                                .icon(Icon::new(LucideIcon::MoveLeft))
                                .accessibility_label("Move tab left")
                                .tooltip("Move tab left")
                                .disabled(selected_index.is_none_or(|index| index == 0))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(id) = this.selected_id() {
                                        this.move_tab(id, -1);
                                        cx.notify();
                                    }
                                })),
                        )
                        .child(
                            Button::new("move-right")
                                .icon(Icon::new(LucideIcon::MoveRight))
                                .accessibility_label("Move tab right")
                                .tooltip("Move tab right")
                                .disabled(selected_index.is_none_or(|index| index + 1 >= tab_count))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(id) = this.selected_id() {
                                        this.move_tab(id, 1);
                                        cx.notify();
                                    }
                                })),
                        ),
                )
                .child(
                    Button::new("app-menu")
                        .outline()
                        .icon(Icon::new(LucideIcon::Settings2))
                        .accessibility_label("Application menu")
                        .tooltip("Settings and About")
                        .dropdown_menu({
                            let menu_entity = entity.clone();
                            move |menu, window, cx| {
                                let theme_entity = menu_entity.clone();
                                let themes = theme_names.clone();
                                let current_theme = selected_theme.clone();
                                let menu = menu.submenu(
                                    format!("Theme: {current_theme}"),
                                    window,
                                    cx,
                                    move |mut submenu, _, _| {
                                        for theme_name in &themes {
                                            let theme_entity = theme_entity.clone();
                                            let theme_name = theme_name.clone();
                                            let checked = theme_name == current_theme;
                                            submenu = submenu.item(
                                                PopupMenuItem::new(theme_name.clone())
                                                    .checked(checked)
                                                    .on_click(move |_, _, app| {
                                                        let _ =
                                                            theme_entity.update(app, |this, cx| {
                                                                this.select_theme(
                                                                    theme_name.clone(),
                                                                    cx,
                                                                );
                                                            });
                                                    }),
                                            );
                                        }
                                        submenu
                                    },
                                );
                                let about_entity = menu_entity.clone();
                                menu.item(
                                    PopupMenuItem::new("About")
                                        .icon(Icon::new(LucideIcon::Info))
                                        .on_click(move |_, _, app| {
                                            let _ = about_entity.update(app, |this, cx| {
                                                this.show_about = true;
                                                cx.notify();
                                            });
                                        }),
                                )
                            }
                        }),
                ),
        )
    }

    fn render_tab_strip(
        &mut self,
        selected_index: Option<usize>,
        tab_count: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity().downgrade();
        let tab_ids = self
            .tabs
            .iter()
            .map(|tab| tab.profile.id)
            .collect::<Vec<_>>();
        let tab_indices = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| (tab.profile.id, index))
            .collect::<HashMap<_, _>>();
        let context_target = self.tab_context_target.clone();
        let tabs = self
            .tabs
            .iter()
            .map(|tab| {
                let id = tab.profile.id;
                let title = if tab.profile.display_name.trim().is_empty() {
                    "CLI".to_owned()
                } else {
                    tab.profile.display_name.clone()
                };
                let tab_context_target = context_target.clone();
                Tab::new()
                    .label(title.clone())
                    .aria_label(title)
                    .disabled(self.active_job.is_some())
                    .on_mouse_down(MouseButton::Right, move |_, _, _| {
                        tab_context_target.set(Some(id));
                    })
            })
            .collect::<Vec<_>>();
        let selected_entity = entity.clone();
        let tab_bar = TabBar::new("cli-profile-tabs")
            .underline()
            .selected_index(selected_index.unwrap_or_default())
            .track_scroll(&self.tab_scroll)
            .max_width(px(220.))
            .menu(true)
            .last_empty_space(div().bg(cx.theme().background))
            .children(tabs)
            .on_click(move |index, _, app| {
                if let Some(id) = tab_ids.get(*index).copied() {
                    let _ = selected_entity.update(app, |this, cx| {
                        this.select_tab(id);
                        cx.notify();
                    });
                }
            })
            .flex_1()
            .min_w_0();
        if self.active_job.is_some() {
            div()
                .id("profile-tab-strip")
                .flex_1()
                .min_w_0()
                .child(tab_bar)
                .into_any_element()
        } else {
            let menu_entity = entity.clone();
            div()
                .id("profile-tab-strip")
                .flex_1()
                .min_w_0()
                .context_menu(move |mut menu, _, _| {
                    let Some(id) = context_target.get() else {
                        return menu;
                    };
                    let Some(index) = tab_indices.get(&id).copied() else {
                        return menu;
                    };
                    let rename_entity = menu_entity.clone();
                    let duplicate_entity = menu_entity.clone();
                    let move_left_entity = menu_entity.clone();
                    let move_right_entity = menu_entity.clone();
                    let delete_entity = menu_entity.clone();
                    menu = menu.item(
                        PopupMenuItem::new("Rename")
                            .icon(Icon::new(LucideIcon::Pencil))
                            .on_click(move |_, window, app| {
                                let _ = rename_entity.update(app, |this, cx| {
                                    this.begin_rename(id, window, cx);
                                });
                            }),
                    );
                    menu = menu.item(
                        PopupMenuItem::new("Duplicate")
                            .icon(Icon::new(LucideIcon::Copy))
                            .on_click(move |_, window, app| {
                                let _ = duplicate_entity.update(app, |this, cx| {
                                    this.duplicate_tab(id, window, cx);
                                });
                            }),
                    );
                    menu = menu.item(
                        PopupMenuItem::new("Move Left")
                            .icon(Icon::new(LucideIcon::MoveLeft))
                            .disabled(index == 0)
                            .on_click(move |_, _, app| {
                                let _ = move_left_entity.update(app, |this, cx| {
                                    this.move_tab(id, -1);
                                    cx.notify();
                                });
                            }),
                    );
                    menu = menu.item(
                        PopupMenuItem::new("Move Right")
                            .icon(Icon::new(LucideIcon::MoveRight))
                            .disabled(index + 1 >= tab_count)
                            .on_click(move |_, _, app| {
                                let _ = move_right_entity.update(app, |this, cx| {
                                    this.move_tab(id, 1);
                                    cx.notify();
                                });
                            }),
                    );
                    menu = menu.item(PopupMenuItem::separator());
                    menu.item(
                        PopupMenuItem::new("Delete")
                            .icon(Icon::new(LucideIcon::Trash))
                            .disabled(tab_count <= 1)
                            .on_click(move |_, _, app| {
                                let _ = delete_entity.update(app, |this, cx| {
                                    this.delete_tab_id = Some(id);
                                    cx.notify();
                                });
                            }),
                    )
                })
                .child(tab_bar)
                .into_any_element()
        }
    }
}
