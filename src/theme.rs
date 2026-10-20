use std::rc::Rc;

use gpui_kit::{
    App,
    component::{Theme, ThemeConfig, ThemeRegistry},
};

use crate::config::DEFAULT_THEME_NAME;

const BUILTIN_THEME_SETS: &[&str] = &[
    include_str!("../assets/themes/catppuccin.json"),
    include_str!("../assets/themes/everforest.json"),
    include_str!("../assets/themes/flexoki.json"),
    include_str!("../assets/themes/gruvbox.json"),
    include_str!("../assets/themes/solarized.json"),
    include_str!("../assets/themes/tokyonight.json"),
];

const APPROVED_THEME_NAMES: &[&str] = &[
    DEFAULT_THEME_NAME,
    "Default Dark",
    "Tokyo Night",
    "Catppuccin Mocha",
    "Everforest Dark",
    "Gruvbox Dark",
    "Solarized Light",
    "Flexoki Light",
];

pub fn load_builtin_themes(cx: &mut App) -> Result<(), String> {
    for theme_set in BUILTIN_THEME_SETS {
        ThemeRegistry::global_mut(cx)
            .load_themes_from_str(theme_set)
            .map_err(|error| format!("Could not load a bundled GPUI Kit theme set: {error}"))?;
    }
    Ok(())
}

pub fn resolve_theme_name(name: &str, cx: &App) -> String {
    if APPROVED_THEME_NAMES.contains(&name) && ThemeRegistry::global(cx).themes().contains_key(name)
    {
        name.to_owned()
    } else {
        DEFAULT_THEME_NAME.to_owned()
    }
}

pub fn find_theme(name: &str, cx: &App) -> Option<Rc<ThemeConfig>> {
    ThemeRegistry::global(cx).themes().get(name).cloned()
}

pub fn theme_names(cx: &App) -> Vec<String> {
    ThemeRegistry::global(cx)
        .sorted_themes()
        .into_iter()
        .map(|theme| theme.name.to_string())
        .filter(|name| APPROVED_THEME_NAMES.contains(&name.as_str()))
        .collect()
}

pub fn apply(name: &str, cx: &mut App) -> bool {
    let Some(theme_config) = find_theme(name, cx) else {
        return false;
    };
    Theme::update(cx, |theme| theme.apply_config(&theme_config));
    true
}
