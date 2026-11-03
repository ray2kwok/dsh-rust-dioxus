//! 三栏骨架：Sidebar / Main / RightBar + 设置浮层 + 状态栏。

use dioxus::prelude::*;

use crate::conversation::ConversationView;
use crate::icons::Icon;
use crate::model::{
    ApprovalItem, ConnectionState, FileEntry, MessageRow, SendMode, SessionSummary, Theme,
    TrajectoryRow,
};
use crate::sidebar::SessionList;

#[component]
pub fn AppShell(
    sessions: Vec<SessionSummary>,
    active_id: Option<String>,
    messages: Vec<MessageRow>,
    connection: ConnectionState,
    #[props(default = Vec::new())] approvals: Vec<ApprovalItem>,
    #[props(default = Vec::new())] trajectory: Vec<TrajectoryRow>,
    #[props(default = Vec::new())] files: Vec<FileEntry>,
    #[props(default = None)] preview: Option<String>,
    #[props(default = 0)] generation: u64,
    #[props(default = String::new())] host_label: String,
    #[props(default = String::new())] active_title: String,
    #[props(default = false)] running: bool,
    #[props(default = SendMode::Queue)] send_mode: SendMode,
    #[props(default = Theme::System)] theme: Theme,
    #[props(default = EventHandler::new(|_| {}))] on_theme: EventHandler<Theme>,
    #[props(default = EventHandler::new(|_| {}))] on_decide: EventHandler<(String, bool)>,
    #[props(default = EventHandler::new(|_| {}))] on_mode: EventHandler<SendMode>,
    #[props(default = EventHandler::new(|_| {}))] on_cancel: EventHandler<()>,
    #[props(default = EventHandler::new(|_| {}))] on_reconnect: EventHandler<()>,
    #[props(default = EventHandler::new(|_| {}))] on_open_file: EventHandler<String>,
    on_select: EventHandler<String>,
    on_send: EventHandler<String>,
) -> Element {
    let mut collapsed = use_signal(|| false);
    let mut show_right = use_signal(|| false);
    let mut show_settings = use_signal(|| false);

    let shell_class = if collapsed() {
        "shell shell--collapsed"
    } else {
        "shell"
    };

    rsx! {
        div { class: "{shell_class}",
            aside { class: "sidebar",
                div { class: "sidebar__head",
                    span { class: "brand", "dsh" }
                    button {
                        class: "icon-btn",
                        title: if collapsed() { "展开侧栏" } else { "收起侧栏" },
                        onclick: move |_| collapsed.toggle(),
                        Icon { glyph: "panel-left-outline".to_string() }
                    }
                }
                if !collapsed() {
                    SessionList {
                        sessions: sessions.clone(),
                        active_id: active_id.clone(),
                        on_select,
                    }
                }
            }

            main { class: "main",
                ConversationView {
                    active_id: active_id.clone(),
                    title: if active_title.is_empty() { "会话".to_string() } else { active_title.clone() },
                    messages,
                    connection,
                    approvals,
                    trajectory,
                    running,
                    send_mode,
                    on_mode,
                    on_cancel,
                    on_decide,
                    on_send,
                    on_toggle_right: move |_| show_right.toggle(),
                }
            }

            if show_right() {
                aside { class: "rightbar",
                    div { class: "rightbar__head",
                        span { "文件" }
                        span { class: "spacer" }
                        button {
                            class: "icon-btn",
                            onclick: move |_| show_right.set(false),
                            Icon { glyph: "close-outline".to_string() }
                        }
                    }
                    div { class: "rightbar__body",
                        if files.is_empty() {
                            div { class: "muted", "目录为空或尚未加载。" }
                        }
                        for entry in files.iter() {
                            {
                                let path = entry.path.clone();
                                rsx! {
                                    button {
                                        key: "{entry.path}",
                                        class: "file-row",
                                        onclick: move |_| on_open_file.call(path.clone()),
                                        span { class: "file-icon",
                                            if entry.is_dir {
                                                Icon { glyph: "chevron-right-outline".to_string(), size: 12.0 }
                                            } else {
                                                Icon { glyph: "chevron-right-outline".to_string(), size: 12.0 }
                                            }
                                        }
                                        span { class: "file-name", "{entry.name}" }
                                    }
                                }
                            }
                        }
                        if let Some(text) = preview.clone() {
                            div { class: "preview",
                                div { class: "preview__head muted", "预览" }
                                pre { class: "preview__body", "{text}" }
                            }
                        }
                    }
                }
            }
        }

        if show_settings() {
            div { class: "modal-backdrop", onclick: move |_| show_settings.set(false),
                div { class: "modal", onclick: move |evt| evt.stop_propagation(),
                    h3 { class: "modal__title", "设置" }
                    div { class: "modal__row",
                        span { class: "modal__label", "主题" }
                        button {
                            class: "ghost-btn",
                            onclick: move |_| on_theme.call(theme.next()),
                            "{theme.label()}"
                        }
                    }
                    div { class: "modal__row",
                        span { class: "modal__label", "Host" }
                        span { class: "muted mono", "{host_label}" }
                    }
                    div { class: "modal__row",
                        span { class: "modal__label", "连接代次" }
                        span { class: "muted", "{generation}" }
                    }
                    div { class: "modal__row",
                        span { class: "modal__label", "会话数" }
                        span { class: "muted", "{sessions.len()}" }
                    }
                    div { class: "modal__actions",
                        button {
                            class: "ghost-btn",
                            onclick: move |_| on_reconnect.call(()),
                            "重新连接"
                        }
                        button {
                            class: "primary-btn",
                            onclick: move |_| show_settings.set(false),
                            "关闭"
                        }
                    }
                }
            }
        }

        footer { class: "statusbar",
            span { class: "{connection.css_class()}", "{connection.label()}" }
            span { class: "muted", "{host_label}" }
            span { class: "spacer" }
            span { class: "muted", "generation #{generation}" }
            button {
                class: "icon-btn",
                title: "设置",
                onclick: move |_| show_settings.set(true),
                Icon { glyph: "settings-outline".to_string() }
            }
        }
    }
}
