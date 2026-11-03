//! 三栏骨架：Sidebar / Main / RightBar + 设置浮层 + 状态栏。

use dioxus::prelude::*;

use crate::conversation::ConversationView;
use crate::i18n::{self, Key, Locale, use_t};
use crate::icons::Icon;
use crate::model::{
    ApprovalItem, ConnectionState, FileEntry, MessageRow, QuestionCard, SendMode, SessionSummary,
    Theme, TrajectoryRow,
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
    #[props(default = EventHandler::new(|_| {}))] on_locale: EventHandler<Locale>,
    #[props(default = Vec::new())] questions: Vec<QuestionCard>,
    #[props(default = EventHandler::new(|_| {}))] on_answer: EventHandler<(
        String,
        Vec<String>,
        String,
    )>,
    #[props(default = EventHandler::new(|_| {}))] on_rename: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_archive: EventHandler<()>,
    #[props(default = EventHandler::new(|_| {}))] on_decide: EventHandler<(String, bool)>,
    #[props(default = EventHandler::new(|_| {}))] on_mode: EventHandler<SendMode>,
    #[props(default = EventHandler::new(|_| {}))] on_cancel: EventHandler<()>,
    #[props(default = EventHandler::new(|_| {}))] on_reconnect: EventHandler<()>,
    #[props(default = EventHandler::new(|_| {}))] on_open_file: EventHandler<String>,
    on_select: EventHandler<String>,
    on_send: EventHandler<String>,
) -> Element {
    let t = use_t();
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
                    questions,
                    on_answer,
                    on_rename,
                    on_archive,
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
                        span { "{t.get(Key::Files)}" }
                        span { class: "spacer" }
                        button {
                            class: "icon-btn",
                            onclick: move |_| show_right.set(false),
                            Icon { glyph: "close-outline".to_string() }
                        }
                    }
                    div { class: "rightbar__body",
                        if files.is_empty() {
                            div { class: "muted", "{t.get(Key::FilesEmpty)}" }
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
                                div { class: "preview__head muted", "{t.get(Key::Preview)}" }
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
                    h3 { class: "modal__title", "{t.get(Key::Settings)}" }
                    div { class: "modal__row",
                        span { class: "modal__label", "{t.get(Key::Theme)}" }
                        button {
                            class: "ghost-btn",
                            onclick: move |_| on_theme.call(theme.next()),
                            "{theme.label()}"
                        }
                    }
                    div { class: "modal__row",
                        span { class: "modal__label", "{t.get(Key::Language)}" }
                        button {
                            class: "ghost-btn",
                            onclick: move |_| on_locale.call(if t.locale() == Locale::Zh { Locale::En } else { Locale::Zh }),
                            "{t.locale().label()}"
                        }
                    }
                    div { class: "modal__row",
                        span { class: "modal__label", "{t.get(Key::Host)}" }
                        span { class: "muted mono", "{host_label}" }
                    }
                    div { class: "modal__row",
                        span { class: "modal__label", "{t.get(Key::Generation)}" }
                        span { class: "muted", "{generation}" }
                    }
                    div { class: "modal__row",
                        span { class: "modal__label", "{t.get(Key::SessionCount)}" }
                        span { class: "muted", "{sessions.len()}" }
                    }
                    div { class: "modal__actions",
                        button {
                            class: "ghost-btn",
                            onclick: move |_| on_reconnect.call(()),
                            "{t.get(Key::Reconnect)}"
                        }
                        button {
                            class: "primary-btn",
                            onclick: move |_| show_settings.set(false),
                            "{t.get(Key::Close)}"
                        }
                    }
                }
            }
        }

        footer { class: "statusbar",
            span { class: "{connection.css_class()}", "{t.get(i18n::connection_key(connection))}" }
            span { class: "muted", "{host_label}" }
            span { class: "spacer" }
            span { class: "muted", "generation #{generation}" }
            button {
                class: "icon-btn",
                title: "{t.get(Key::Settings)}",
                onclick: move |_| show_settings.set(true),
                Icon { glyph: "settings-outline".to_string() }
            }
        }
    }
}
