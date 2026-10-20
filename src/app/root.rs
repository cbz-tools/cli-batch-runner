use super::*;
use gpui_kit::AnyElement;

impl BatchRunner {
    pub(super) fn render_root_main_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let settings_mode = self.settings_mode_active();
        let mut children = vec![
            TextSelectionLayer.into_any_element(),
            self.render_header(window, cx).into_any_element(),
            self.render_settings(window, cx).into_any_element(),
        ];
        if !settings_mode {
            children.push(self.render_queue(window, cx).into_any_element());
            children.push(self.render_progress(cx).into_any_element());
        }
        children
    }

    pub(super) fn render_root_feedback(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut children = Vec::new();
        if let Some(message) = self.banner.clone() {
            children.push(
                div()
                    .id("banner")
                    .w_full()
                    .p_2()
                    .bg(cx.theme().muted)
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_color(cx.theme().danger)
                    .child(message)
                    .child(
                        Button::new("banner-dismiss")
                            .tooltip("Dismiss notice")
                            .label("×")
                            .accessibility_label("Dismiss notice")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.banner = None;
                                cx.notify();
                            })),
                    )
                    .into_any_element(),
            );
        }
        if self.show_help {
            children.push(self.render_help_dialog(cx).into_any_element());
        }
        if self.show_about {
            children.push(self.render_about_dialog(cx).into_any_element());
        }
        if self.delete_tab_id.is_some() {
            children.push(self.render_delete_dialog(cx).into_any_element());
        }
        if self.rename_tab_id.is_some() {
            children.push(self.render_rename_dialog(cx).into_any_element());
        }
        if let Some((tab_id, file_id)) = self.log_dialog {
            children.push(
                self.render_log_dialog(tab_id, file_id, cx)
                    .into_any_element(),
            );
        }
        children
    }
}
