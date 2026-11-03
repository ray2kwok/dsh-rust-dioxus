//! 输入区：文本、附件、`@` 引用、`/` 命令、模型与权限选择。
//!
//! 文件由 Dioxus desktop 的原生 `<input type="file">` 选择，字节随
//! [`SendRequest`] 交给 App；App 负责图片内联、文件上传与 `/` 命令执行。

use std::time::{SystemTime, UNIX_EPOCH};

use dioxus::prelude::*;

use crate::i18n::{Key, use_t};
use crate::icons::Icon;
use crate::model::{ComposerAttachment, PlanState, SendMode, SendRequest};
use crate::settings::{ModelItemView, PermissionItemView};

#[derive(Debug, Clone, PartialEq)]
pub struct CommandItemView {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileRefItemView {
    pub path: String,
    pub is_dir: bool,
}

#[component]
pub fn Composer(
    can_send: bool,
    running: bool,
    #[props(default = SendMode::Queue)] send_mode: SendMode,
    #[props(default = None)] notice: Option<String>,
    #[props(default = false)] can_stop: bool,
    #[props(default = Vec::new())] commands: Vec<CommandItemView>,
    #[props(default = Vec::new())] file_refs: Vec<FileRefItemView>,
    #[props(default = Vec::new())] models: Vec<ModelItemView>,
    #[props(default = None)] selected_model: Option<(String, String)>,
    #[props(default = Vec::new())] permissions: Vec<PermissionItemView>,
    #[props(default = None)] selected_permission: Option<String>,
    #[props(default = PlanState::default())] plan: PlanState,
    on_send: EventHandler<SendRequest>,
    #[props(default = EventHandler::new(|_| {}))] on_mode: EventHandler<SendMode>,
    #[props(default = EventHandler::new(|_| {}))] on_cancel: EventHandler<()>,
    #[props(default = EventHandler::new(|_| {}))] on_model: EventHandler<(String, String)>,
    #[props(default = EventHandler::new(|_| {}))] on_permission: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_command_query: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_file_query: EventHandler<String>,
) -> Element {
    let t = use_t();
    let mut draft = use_signal(String::new);
    let mut attachments = use_signal(Vec::<ComposerAttachment>::new);
    let mut menu = use_signal(|| None::<ComposerMenu>);
    let mut uploading = use_signal(|| false);

    if let Some(notice) = notice {
        return rsx! {
            footer { class: "composer composer--readonly",
                div { class: "composer__notice", "{notice}" }
                if running && can_stop {
                    button {
                        class: "ghost-btn ghost-btn--danger",
                        onclick: move |_| on_cancel.call(()),
                        "{t.get(Key::Stop)}"
                    }
                }
            }
        };
    }

    let text = draft();
    let command_query = text
        .strip_prefix('/')
        .map(|rest| rest.split_whitespace().next().unwrap_or("").to_string());
    let file_query = text
        .rsplit_once('@')
        .filter(|(_, tail)| !tail.contains(char::is_whitespace))
        .map(|(_, tail)| tail.to_string());

    let command_matches: Vec<CommandItemView> = command_query
        .as_deref()
        .map(|query| {
            commands
                .iter()
                .filter(|item| {
                    query.is_empty()
                        || item
                            .name
                            .to_ascii_lowercase()
                            .contains(&query.to_ascii_lowercase())
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let file_matches: Vec<FileRefItemView> = file_query
        .as_deref()
        .map(|query| {
            file_refs
                .iter()
                .filter(|item| {
                    query.is_empty()
                        || item
                            .path
                            .to_ascii_lowercase()
                            .contains(&query.to_ascii_lowercase())
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    let selected_model_label = selected_model
        .as_ref()
        .and_then(|(provider, model)| {
            models
                .iter()
                .find(|item| &item.provider == provider && &item.model == model)
                .map(|item| item.name.clone())
        })
        .unwrap_or_else(|| t.tr("模型", "Model").to_string());
    let selected_permission_label = selected_permission
        .as_ref()
        .and_then(|value| {
            permissions
                .iter()
                .find(|item| &item.value == value)
                .map(|item| item.name.clone())
        })
        .unwrap_or_else(|| t.tr("权限", "Permission").to_string());

    rsx! {
        footer { class: "composer",
            if plan.active || plan.pending {
                div { class: "composer__plan",
                    span { class: "chip chip--plan",
                        if plan.pending {
                            "{t.tr(\"计划模式（等待切换）\", \"Plan mode (pending)\")}"
                        } else {
                            "{t.tr(\"计划模式\", \"Plan mode\")}"
                        }
                    }
                }
            }
            if !attachments.read().is_empty() {
                div { class: "composer__attachments",
                    for attachment in attachments.read().iter() {
                        {
                            let id = attachment.id.clone();
                            rsx! {
                                span { key: "{attachment.id}", class: "attachment-chip",
                                    if attachment.is_image() {
                                        Icon { glyph: "image-outline".to_string(), size: 12.0 }
                                    } else {
                                        Icon { glyph: "file-outline".to_string(), size: 12.0 }
                                    }
                                    span { class: "attachment-chip__name", "{attachment.name}" }
                                    button {
                                        class: "icon-btn attachment-chip__remove",
                                        onclick: move |_| attachments.write().retain(|item| item.id != id),
                                        Icon { glyph: "close-outline".to_string(), size: 10.0 }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            textarea {
                class: "composer__input",
                rows: "3",
                placeholder: if can_send { t.get(Key::ComposerPlaceholder) } else { t.get(Key::ComposerDisabled) },
                disabled: !can_send,
                value: "{draft}",
                oninput: move |evt| {
                    let value = evt.value();
                    draft.set(value.clone());
                    if let Some(query) = value.strip_prefix('/') {
                        let query = query.split_whitespace().next().unwrap_or("");
                        on_command_query.call(query.to_string());
                        menu.set(Some(ComposerMenu::Commands));
                    } else if let Some((_, tail)) = value.rsplit_once('@')
                        && !tail.contains(char::is_whitespace)
                    {
                        on_file_query.call(tail.to_string());
                        menu.set(Some(ComposerMenu::Files));
                    } else if !matches!(menu(), Some(ComposerMenu::Models | ComposerMenu::Permissions)) {
                        menu.set(None);
                    }
                },
                onkeydown: move |evt| {
                    if evt.key().to_string() == "Escape" {
                        menu.set(None);
                        return;
                    }
                    if evt.key().to_string() == "Enter" && !evt.modifiers().shift() {
                        evt.prevent_default();
                        let text = draft();
                        if text.trim().is_empty() && attachments.read().is_empty() {
                            return;
                        }
                        on_send.call(SendRequest {
                            text,
                            attachments: attachments.read().clone(),
                        });
                        draft.set(String::new());
                        attachments.set(Vec::new());
                        menu.set(None);
                    }
                },
            }
            if matches!(menu(), Some(ComposerMenu::Commands)) && !command_matches.is_empty() {
                div { class: "composer-popover composer-popover--commands",
                    for command in command_matches.iter() {
                        button {
                            key: "{command.name}",
                            class: "composer-popover__item",
                            onclick: {
                                let name = command.name.clone();
                                move |_| {
                                    draft.set(format!("/{name} "));
                                    menu.set(None);
                                }
                            },
                            span { class: "mono", "/{command.name}" }
                            span { class: "muted", "{command.description}" }
                        }
                    }
                }
            }
            if matches!(menu(), Some(ComposerMenu::Files)) && !file_matches.is_empty() {
                div { class: "composer-popover composer-popover--files",
                    for item in file_matches.clone() {
                        button {
                            key: "{item.path}",
                            class: "composer-popover__item",
                            onclick: {
                                let path = item.path.clone();
                                move |_| {
                                    let mut value = draft();
                                    if let Some(pos) = value.rfind('@') {
                                        value.truncate(pos);
                                        value.push('@');
                                        value.push_str(&path);
                                        if !item.is_dir {
                                            value.push(' ');
                                        }
                                    }
                                    draft.set(value);
                                    menu.set(None);
                                }
                            },
                            span { class: "mono", "{item.path}" }
                            if item.is_dir {
                                span { class: "muted", "dir" }
                            }
                        }
                    }
                }
            }

            div { class: "composer__bar",
                label { class: "icon-btn composer__attach", title: t.tr("添加附件", "Attach file"),
                    Icon { glyph: "plus-outline".to_string(), size: 14.0 }
                    input {
                        r#type: "file",
                        multiple: true,
                        class: "composer__file-input",
                        onchange: move |evt| {
                            let files = evt.files();
                            if files.is_empty() {
                                return;
                            }
                            uploading.set(true);
                            spawn(async move {
                                let mut pending = attachments.read().clone();
                                for file in files {
                                    let name = file.name();
                                    let media_type = file
                                        .content_type()
                                        .unwrap_or_else(|| guess_media_type(&name).to_string());
                                    if let Ok(bytes) = file.read_bytes().await {
                                        let nonce = SystemTime::now()
                                            .duration_since(UNIX_EPOCH)
                                            .map(|value| value.as_nanos())
                                            .unwrap_or_default();
                                        pending.push(ComposerAttachment {
                                            id: format!("att-{nonce}-{}", pending.len()),
                                            name,
                                            media_type,
                                            bytes: bytes.to_vec(),
                                        });
                                    }
                                }
                                attachments.set(pending);
                                uploading.set(false);
                            });
                        },
                    }
                }
                if uploading() {
                    span { class: "muted", "…" }
                }
                button {
                    class: "chip chip--model",
                    title: t.tr("选择模型", "Select model"),
                    onclick: move |_| {
                        if matches!(menu(), Some(ComposerMenu::Models)) { menu.set(None); }
                        else { menu.set(Some(ComposerMenu::Models)); }
                    },
                    "{selected_model_label}"
                }
                button {
                    class: "chip chip--permission",
                    title: t.tr("权限预设", "Permission preset"),
                    onclick: move |_| {
                        if matches!(menu(), Some(ComposerMenu::Permissions)) { menu.set(None); }
                        else { menu.set(Some(ComposerMenu::Permissions)); }
                    },
                    "{selected_permission_label}"
                }
                span { class: "spacer" }
                div { class: "segmented",
                    button {
                        class: "{send_mode.css_class(send_mode == SendMode::Queue)}",
                        onclick: move |_| on_mode.call(SendMode::Queue),
                        "{t.get(Key::Queue)}"
                    }
                    button {
                        class: "{send_mode.css_class(send_mode == SendMode::Steer)}",
                        onclick: move |_| on_mode.call(SendMode::Steer),
                        "{t.get(Key::Steer)}"
                    }
                }
                if running {
                    button {
                        class: "ghost-btn ghost-btn--danger",
                        onclick: move |_| on_cancel.call(()),
                        "{t.get(Key::Stop)}"
                    }
                }
                button {
                    class: "primary-btn",
                    disabled: !can_send || (draft().trim().is_empty() && attachments.read().is_empty()),
                    onclick: move |_| {
                        let text = draft();
                        if text.trim().is_empty() && attachments.read().is_empty() {
                            return;
                        }
                        on_send.call(SendRequest {
                            text,
                            attachments: attachments.read().clone(),
                        });
                        draft.set(String::new());
                        attachments.set(Vec::new());
                        menu.set(None);
                    },
                    "{t.get(Key::Send)}"
                }
            }

            if matches!(menu(), Some(ComposerMenu::Models)) {
                div { class: "composer-popover composer-popover--models",
                    for model in models.iter() {
                        button {
                            key: "{model.provider}-{model.model}",
                            class: if selected_model.as_ref().is_some_and(|(p, m)| p == &model.provider && m == &model.model) {
                                "composer-popover__item composer-popover__item--active"
                            } else {
                                "composer-popover__item"
                            },
                            onclick: {
                                let provider = model.provider.clone();
                                let id = model.model.clone();
                                move |_| {
                                    on_model.call((provider.clone(), id.clone()));
                                    menu.set(None);
                                }
                            },
                            span { "{model.name}" }
                            span { class: "muted mono", "{model.model}" }
                        }
                    }
                }
            }
            if matches!(menu(), Some(ComposerMenu::Permissions)) {
                div { class: "composer-popover composer-popover--permissions",
                    for permission in permissions.iter() {
                        button {
                            key: "{permission.value}",
                            class: if selected_permission.as_deref() == Some(permission.value.as_str()) {
                                "composer-popover__item composer-popover__item--active"
                            } else {
                                "composer-popover__item"
                            },
                            title: permission.description.clone().unwrap_or_default(),
                            onclick: {
                                let value = permission.value.clone();
                                move |_| {
                                    on_permission.call(value.clone());
                                    menu.set(None);
                                }
                            },
                            span { "{permission.name}" }
                            if let Some(description) = permission.description.clone() {
                                span { class: "muted", "{description}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComposerMenu {
    Commands,
    Files,
    Models,
    Permissions,
}

fn guess_media_type(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "md" => "text/markdown",
        "txt" => "text/plain",
        "rs" => "text/plain",
        "ts" | "tsx" | "js" | "jsx" => "text/plain",
        "py" => "text/plain",
        "sh" => "text/plain",
        _ => "application/octet-stream",
    }
}
