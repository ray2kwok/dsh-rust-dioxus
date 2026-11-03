//! 设置页：General / Models / Agents / About。
//!
//! 组件只发意图；读写 Host `settings/*`、模型切换与 preset 应用由 App 完成。

use dioxus::prelude::*;

use crate::i18n::{Locale, use_t};
use crate::icons::Icon;
use crate::model::Theme;

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsSnapshot {
    pub writable: bool,
    pub theme: Theme,
    pub locale: Locale,
    pub font_size: u64,
    pub code_font_size: u64,
    pub text_font_family: String,
    pub code_font_family: String,
    pub max_depth: Option<u64>,
    pub max_active_subagents: Option<u64>,
    pub theme_revision: Option<u64>,
    pub locale_revision: Option<u64>,
    pub subagent_revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SettingsChange {
    Theme(Theme),
    Locale(Locale),
    FontSize(u64),
    CodeFontSize(u64),
    TextFontFamily(String),
    CodeFontFamily(String),
    MaxDepth(u64),
    MaxActiveSubagents(u64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelItemView {
    pub provider: String,
    pub model: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelGroupView {
    pub id: String,
    pub name: String,
    pub models: Vec<ModelItemView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PermissionItemView {
    pub value: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentPresetItemView {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_default: bool,
    pub broken: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    General,
    Models,
    Agents,
    About,
}

#[component]
pub fn SettingsPanel(
    snapshot: Option<SettingsSnapshot>,
    #[props(default = Vec::new())] model_groups: Vec<ModelGroupView>,
    #[props(default = Vec::new())] permissions: Vec<PermissionItemView>,
    #[props(default = Vec::new())] agent_presets: Vec<AgentPresetItemView>,
    #[props(default = None)] selected_model: Option<(String, String)>,
    #[props(default = None)] selected_permission: Option<String>,
    #[props(default = String::new())] host_label: String,
    #[props(default = 0)] generation: u64,
    #[props(default = false)] saving: bool,
    on_change: EventHandler<SettingsChange>,
    #[props(default = EventHandler::new(|_| {}))] on_select_model: EventHandler<(String, String)>,
    #[props(default = EventHandler::new(|_| {}))] on_select_permission: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_apply_preset: EventHandler<String>,
    on_close: EventHandler<()>,
) -> Element {
    let t = use_t();
    let mut section = use_signal(|| Section::General);

    let Some(snapshot) = snapshot else {
        return rsx! {
            div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
                div { class: "modal settings-modal", onclick: move |evt| evt.stop_propagation(),
                    h3 { class: "modal__title", "{t.tr(\"设置\", \"Settings\")}" }
                    div { class: "muted", "{t.tr(\"正在读取 Host 设置…\", \"Reading Host settings…\")}" }
                    div { class: "modal__actions",
                        button { class: "primary-btn", onclick: move |_| on_close.call(()),
                            "{t.tr(\"关闭\", \"Close\")}"
                        }
                    }
                }
            }
        };
    };

    let tab = |id: Section, label: &'static str| {
        let active = section() == id;
        rsx! {
            button {
                class: if active { "settings-tab settings-tab--active" } else { "settings-tab" },
                onclick: move |_| section.set(id),
                "{label}"
            }
        }
    };

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            div { class: "modal settings-modal", onclick: move |evt| evt.stop_propagation(),
                header { class: "settings-head",
                    h3 { class: "modal__title", "{t.tr(\"设置\", \"Settings\")}" }
                    if saving {
                        span { class: "chip chip--live", "{t.tr(\"保存中\", \"Saving\")}" }
                    }
                    span { class: "spacer" }
                    button { class: "icon-btn", onclick: move |_| on_close.call(()),
                        Icon { glyph: "close-outline".to_string(), size: 14.0 }
                    }
                }
                div { class: "settings-tabs",
                    {tab(Section::General, t.tr("通用", "General"))}
                    {tab(Section::Models, t.tr("模型", "Models"))}
                    {tab(Section::Agents, t.tr("Agent Presets", "Agent presets"))}
                    {tab(Section::About, t.tr("关于", "About"))}
                }
                div { class: "settings-body",
                    match section() {
                        Section::General => rsx! {
                            div { class: "settings-group",
                                div { class: "settings-label", "{t.tr(\"外观\", \"Appearance\")}" }
                                div { class: "settings-options",
                                    for theme in [Theme::Light, Theme::Dark, Theme::System] {
                                        button {
                                            class: if snapshot.theme == theme { "seg seg--active" } else { "seg" },
                                            disabled: !snapshot.writable,
                                            onclick: move |_| on_change.call(SettingsChange::Theme(theme)),
                                            "{theme_label(t, theme)}"
                                        }
                                    }
                                }
                            }
                            div { class: "settings-group",
                                div { class: "settings-label", "{t.tr(\"语言\", \"Language\")}" }
                                div { class: "settings-options",
                                    for locale in [Locale::Zh, Locale::En] {
                                        button {
                                            class: if snapshot.locale == locale { "seg seg--active" } else { "seg" },
                                            disabled: !snapshot.writable,
                                            onclick: move |_| on_change.call(SettingsChange::Locale(locale)),
                                            "{locale.label()}"
                                        }
                                    }
                                }
                            }
                            div { class: "settings-group",
                                div { class: "settings-label", "{t.tr(\"字号\", \"Font size\")}" }
                                div { class: "settings-row",
                                    span { class: "settings-hint", "{t.tr(\"正文\", \"Text\")}" }
                                    button {
                                        class: "icon-btn",
                                        disabled: !snapshot.writable || snapshot.font_size <= 10,
                                        onclick: {
                                            let value = snapshot.font_size.saturating_sub(1).max(10);
                                            move |_| on_change.call(SettingsChange::FontSize(value))
                                        },
                                        "−"
                                    }
                                    span { class: "settings-value", "{snapshot.font_size}px" }
                                    button {
                                        class: "icon-btn",
                                        disabled: !snapshot.writable || snapshot.font_size >= 22,
                                        onclick: {
                                            let value = (snapshot.font_size + 1).min(22);
                                            move |_| on_change.call(SettingsChange::FontSize(value))
                                        },
                                        "+"
                                    }
                                }
                                div { class: "settings-row",
                                    span { class: "settings-hint", "{t.tr(\"代码\", \"Code\")}" }
                                    button {
                                        class: "icon-btn",
                                        disabled: !snapshot.writable || snapshot.code_font_size <= 10,
                                        onclick: {
                                            let value = snapshot.code_font_size.saturating_sub(1).max(10);
                                            move |_| on_change.call(SettingsChange::CodeFontSize(value))
                                        },
                                        "−"
                                    }
                                    span { class: "settings-value", "{snapshot.code_font_size}px" }
                                    button {
                                        class: "icon-btn",
                                        disabled: !snapshot.writable || snapshot.code_font_size >= 16,
                                        onclick: {
                                            let value = (snapshot.code_font_size + 1).min(16);
                                            move |_| on_change.call(SettingsChange::CodeFontSize(value))
                                        },
                                        "+"
                                    }
                                }
                            }
                            div { class: "settings-group",
                                div { class: "settings-label", "{t.tr(\"字体族\", \"Font families\")}" }
                                label { class: "settings-field",
                                    span { "{t.tr(\"正文字体\", \"Text font\")}" }
                                    input {
                                        class: "settings-input",
                                        value: "{snapshot.text_font_family}",
                                        placeholder: t.tr("留空使用内置字体", "Empty uses the built-in stack"),
                                        disabled: !snapshot.writable,
                                        onchange: move |evt| on_change.call(SettingsChange::TextFontFamily(evt.value())),
                                    }
                                }
                                label { class: "settings-field",
                                    span { "{t.tr(\"代码字体\", \"Code font\")}" }
                                    input {
                                        class: "settings-input",
                                        value: "{snapshot.code_font_family}",
                                        placeholder: t.tr("留空使用内置字体", "Empty uses the built-in stack"),
                                        disabled: !snapshot.writable,
                                        onchange: move |evt| on_change.call(SettingsChange::CodeFontFamily(evt.value())),
                                    }
                                }
                            }
                            if snapshot.max_depth.is_some() || snapshot.max_active_subagents.is_some() {
                                div { class: "settings-group",
                                    div { class: "settings-label", "{t.tr(\"子 Agent 委派\", \"Subagent delegation\")}" }
                                    div { class: "settings-row",
                                        span { class: "settings-hint", "{t.tr(\"最大深度\", \"Max depth\")}" }
                                        input {
                                            class: "settings-input settings-input--num",
                                            r#type: "number",
                                            min: "0",
                                            value: "{snapshot.max_depth.unwrap_or(0)}",
                                            disabled: !snapshot.writable,
                                            onchange: move |evt| {
                                                if let Ok(value) = evt.value().parse::<u64>() {
                                                    on_change.call(SettingsChange::MaxDepth(value));
                                                }
                                            },
                                        }
                                    }
                                    div { class: "settings-row",
                                        span { class: "settings-hint", "{t.tr(\"最大并发\", \"Max active\")}" }
                                        input {
                                            class: "settings-input settings-input--num",
                                            r#type: "number",
                                            min: "0",
                                            value: "{snapshot.max_active_subagents.unwrap_or(0)}",
                                            disabled: !snapshot.writable,
                                            onchange: move |evt| {
                                                if let Ok(value) = evt.value().parse::<u64>() {
                                                    on_change.call(SettingsChange::MaxActiveSubagents(value));
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                            div { class: "settings-group",
                                div { class: "settings-label", "{t.tr(\"权限预设\", \"Permission preset\")}" }
                                div { class: "settings-options settings-options--wrap",
                                    for permission in permissions.iter() {
                                        button {
                                            key: "{permission.value}",
                                            class: if selected_permission.as_deref() == Some(permission.value.as_str()) { "seg seg--active" } else { "seg" },
                                            title: permission.description.clone().unwrap_or_default(),
                                            onclick: {
                                                let value = permission.value.clone();
                                                move |_| on_select_permission.call(value.clone())
                                            },
                                            "{permission.name}"
                                        }
                                    }
                                }
                            }
                        },
                        Section::Models => rsx! {
                            if model_groups.is_empty() {
                                div { class: "muted", "{t.tr(\"Host 未返回可用模型。\", \"No models reported by the Host.\")}" }
                            }
                            for group in model_groups.iter() {
                                div { key: "{group.id}", class: "settings-group",
                                    div { class: "settings-label", "{group.name}" }
                                    for model in group.models.iter() {
                                        {
                                            let selected = selected_model.as_ref().is_some_and(|(provider, id)| {
                                                provider == &model.provider && id == &model.model
                                            });
                                            let provider = model.provider.clone();
                                            let id = model.model.clone();
                                            rsx! {
                                                button {
                                                    key: "{model.model}",
                                                    class: if selected { "model-row model-row--selected" } else { "model-row" },
                                                    onclick: move |_| on_select_model.call((provider.clone(), id.clone())),
                                                    span { class: "model-row__name", "{model.name}" }
                                                    span { class: "model-row__id muted mono", "{model.model}" }
                                                    if selected {
                                                        span { class: "chip chip--ok", "{t.tr(\"当前\", \"Current\")}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        Section::Agents => rsx! {
                            if agent_presets.is_empty() {
                                div { class: "muted", "{t.tr(\"Host 未返回 Agent preset。\", \"No Agent presets reported by the Host.\")}" }
                            }
                            for preset in agent_presets.iter() {
                                div { key: "{preset.id}", class: "settings-group",
                                    div { class: "settings-row",
                                        div { class: "settings-preset",
                                            span { class: "settings-preset__name",
                                                "{preset.name}"
                                                if preset.is_default {
                                                    span { class: "chip chip--ok", "{t.tr(\"默认\", \"Default\")}" }
                                                }
                                            }
                                            if let Some(description) = preset.description.clone() {
                                                span { class: "settings-hint", "{description}" }
                                            }
                                            if let Some(broken) = preset.broken.clone() {
                                                span { class: "settings-error", "{broken}" }
                                            }
                                        }
                                        span { class: "spacer" }
                                        button {
                                            class: "ghost-btn",
                                            disabled: preset.broken.is_some(),
                                            onclick: {
                                                let id = preset.id.clone();
                                                move |_| on_apply_preset.call(id.clone())
                                            },
                                            "{t.tr(\"应用并设为默认\", \"Apply & default\")}"
                                        }
                                    }
                                }
                            }
                        },
                        Section::About => rsx! {
                            div { class: "settings-group",
                                div { class: "settings-row",
                                    span { class: "settings-hint", "Host" }
                                    span { class: "muted mono", "{host_label}" }
                                }
                                div { class: "settings-row",
                                    span { class: "settings-hint", "Generation" }
                                    span { class: "muted", "#{generation}" }
                                }
                                div { class: "settings-row",
                                    span { class: "settings-hint", "{t.tr(\"版本\", \"Version\")}" }
                                    span { class: "muted mono", "{env!(\"CARGO_PKG_VERSION\")}" }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

fn theme_label(t: crate::i18n::T, theme: Theme) -> &'static str {
    match theme {
        Theme::Light => t.tr("浅色", "Light"),
        Theme::Dark => t.tr("深色", "Dark"),
        Theme::System => t.tr("跟随系统", "System"),
    }
}
