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

/// 一条可配置的模型提供方。
///
/// `key_ref` 是凭据引用（通常是 API Key 的环境变量名）：
/// 官方实现取该命名空间自身的 `apiKeyEnv`，pi-ai route 取 `nativeAuth.env[0]`。
/// `None` 表示该提供方只能走登录流程，这里不提供填写入口。
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderRow {
    pub id: String,
    pub name: String,
    pub key_ref: Option<String>,
    pub configured: bool,
    pub writable: bool,
    pub active: bool,
    /// 写入 route 用的设置命名空间（官方实现为 `llm-deepseek`，pi-ai 为 `llm-pi-ai`）。
    pub settings_ns: String,
    /// 空表示官方实现；否则是 `["providers", "<route>"]`。
    pub settings_path: Vec<String>,
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
    #[props(default = Vec::new())] providers: Vec<ProviderRow>,
    #[props(default = Vec::new())] permissions: Vec<PermissionItemView>,
    #[props(default = Vec::new())] agent_presets: Vec<AgentPresetItemView>,
    #[props(default = None)] selected_model: Option<(String, String)>,
    #[props(default = None)] selected_permission: Option<String>,
    #[props(default = String::new())] host_label: String,
    #[props(default = 0)] generation: u64,
    #[props(default = false)] saving: bool,
    on_change: EventHandler<SettingsChange>,
    #[props(default = EventHandler::new(|_| {}))] on_select_model: EventHandler<(String, String)>,
    #[props(default = EventHandler::new(|_| {}))] on_provider_save: EventHandler<(String, String)>,
    #[props(default = EventHandler::new(|_| {}))] on_provider_remove: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_select_permission: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_apply_preset: EventHandler<String>,
    on_close: EventHandler<()>,
) -> Element {
    let t = use_t();
    let mut section = use_signal(|| Section::General);
    let mut provider_choice = use_signal(String::new);
    let mut provider_key = use_signal(String::new);

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

    // 下拉默认落在第一个「可填写凭据」的提供方上，避免空选。
    let first_choice = providers
        .iter()
        .find(|item| item.key_ref.is_some())
        .map(|item| item.id.clone())
        .unwrap_or_default();
    let chosen = provider_choice();
    let choice_id = if chosen.is_empty() {
        first_choice
    } else {
        chosen
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
                            div { class: "settings-group",
                                div { class: "settings-label", "{t.tr(\"模型提供方\", \"Model providers\")}" }
                                div { class: "settings-hint",
                                    "{t.tr(\"填写 API Key 后，Host 会返回该提供方的模型。\", \"The Host lists a provider's models once its API key is set.\")}"
                                }
                                div { class: "settings-row",
                                    select {
                                        class: "settings-input",
                                        value: "{choice_id}",
                                        onchange: move |evt| provider_choice.set(evt.value()),
                                        for provider in providers.iter().filter(|item| item.key_ref.is_some()) {
                                            option {
                                                key: "{provider.id}",
                                                value: "{provider.id}",
                                                selected: provider.id == choice_id,
                                                "{provider.name}"
                                            }
                                        }
                                    }
                                    input {
                                        class: "settings-input",
                                        r#type: "password",
                                        placeholder: t.tr("API Key", "API key"),
                                        value: "{provider_key()}",
                                        oninput: move |evt| provider_key.set(evt.value()),
                                    }
                                    button {
                                        class: "primary-btn",
                                        disabled: saving || choice_id.is_empty(),
                                        onclick: {
                                            let id = choice_id.clone();
                                            move |_| {
                                                let key = provider_key.read().trim().to_string();
                                                if key.is_empty() {
                                                    return;
                                                }
                                                on_provider_save.call((id.clone(), key));
                                                provider_key.set(String::new());
                                            }
                                        },
                                        "{t.tr(\"保存\", \"Save\")}"
                                    }
                                }
                                for provider in providers.iter().filter(|item| item.configured) {
                                    div { key: "{provider.id}", class: "settings-row",
                                        span { class: "settings-hint", "{provider.name}" }
                                        span { class: "muted mono", "{provider.id}" }
                                        if let Some(reference) = provider.key_ref.clone() {
                                            span { class: "muted mono", "{reference}" }
                                        }
                                        span { class: "spacer" }
                                        span { class: "chip chip--ok", "{t.tr(\"已配置\", \"Configured\")}" }
                                        button {
                                            class: "ghost-btn",
                                            onclick: {
                                                let id = provider.id.clone();
                                                move |_| on_provider_remove.call(id.clone())
                                            },
                                            "{t.tr(\"清除\", \"Remove\")}"
                                        }
                                    }
                                }
                            }
                            div { class: "settings-group",
                                div { class: "settings-label", "{t.tr(\"模型\", \"Models\")}" }
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
