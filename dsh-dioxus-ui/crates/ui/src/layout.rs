//! 三栏骨架：Sidebar / Main / RightBar + 设置浮层 + 状态栏。

use std::collections::HashMap;

use dioxus::prelude::*;

use crate::conversation::ConversationView;
use crate::i18n::{self, Key, Locale, use_t};
use crate::icons::Icon;
use crate::model::{
    ApprovalItem, AsideChat, ConnectionState, FileEntry, MessageRow, QuestionCard, QueueItem,
    SendMode, SessionSummary, SubagentMode, SubagentRow, Theme, TrajectoryRow,
};
use crate::sidebar::SessionList;
use crate::subagent::SubagentAside;

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
    #[props(default = Vec::new())] queue: Vec<QueueItem>,
    #[props(default = None)] context_usage: Option<(u64, u64)>,
    #[props(default = EventHandler::new(|_| {}))] on_queue_action: EventHandler<(String, bool)>,
    #[props(default = false)] has_more: bool,
    #[props(default = EventHandler::new(|_| {}))] on_load_earlier: EventHandler<()>,
    #[props(default = EventHandler::new(|_| {}))] on_decide: EventHandler<(String, bool)>,
    #[props(default = EventHandler::new(|_| {}))] on_mode: EventHandler<SendMode>,
    #[props(default = EventHandler::new(|_| {}))] on_cancel: EventHandler<()>,
    #[props(default = EventHandler::new(|_| {}))] on_reconnect: EventHandler<()>,
    #[props(default = EventHandler::new(|_| {}))] on_open_file: EventHandler<String>,
    on_select: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_new_session: EventHandler<()>,
    on_send: EventHandler<String>,
    #[props(default = false)] is_subagent: bool,
    #[props(default = None)] subagent_mode: Option<SubagentMode>,
    #[props(default = true)] parent_available: bool,
    #[props(default = Vec::new())] lineage: Vec<(String, String)>,
    #[props(default = EventHandler::new(|_| {}))] on_navigate: EventHandler<String>,
    #[props(default = Vec::new())] subagent_rows: Vec<SubagentRow>,
    #[props(default = HashMap::new())] subagent_tree: HashMap<String, Vec<SubagentRow>>,
    #[props(default = Vec::new())] subagent_expanded: Vec<String>,
    #[props(default = EventHandler::new(|_| {}))] on_subagent_toggle: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_subagent_open: EventHandler<SubagentRow>,
    #[props(default = EventHandler::new(|_| {}))] on_subagent_aside: EventHandler<SubagentRow>,
    #[props(default = EventHandler::new(|_| {}))] on_subagent_refresh: EventHandler<()>,
    #[props(default = None)] aside: Option<AsideChat>,
    #[props(default = EventHandler::new(|_| {}))] on_close_aside: EventHandler<()>,
) -> Element {
    let t = use_t();
    let mut collapsed = use_signal(|| false);
    let mut show_right = use_signal(|| false);
    let mut show_settings = use_signal(|| false);
    let aside_open = aside.is_some();

    let shell_class = {
        let mut classes = String::from("shell");
        if collapsed() {
            classes.push_str(" shell--collapsed");
        }
        if show_right() || aside_open {
            classes.push_str(" shell--rightbar");
        }
        classes
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
                        on_new: on_new_session,
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
                    queue,
                    context_usage,
                    has_more,
                    on_load_earlier,
                    on_queue_action,
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
                    is_subagent,
                    subagent_mode,
                    parent_available,
                    lineage: lineage.clone(),
                    on_navigate,
                    subagent_rows: subagent_rows.clone(),
                    subagent_tree: subagent_tree.clone(),
                    subagent_expanded: subagent_expanded.clone(),
                    on_subagent_toggle,
                    on_subagent_open,
                    on_subagent_aside,
                    on_subagent_refresh,
                    on_toggle_right: move |_| {
                        if show_right() || aside_open {
                            show_right.set(false);
                            on_close_aside.call(());
                        } else {
                            show_right.set(true);
                        }
                    },
                }
            }

            if show_right() || aside.is_some() {
                aside { class: "rightbar",
                    div { class: "rightbar__head",
                        span { "{t.get(Key::Files)}" }
                        span { class: "spacer" }
                        button {
                            class: "icon-btn",
                            onclick: move |_| {
                                show_right.set(false);
                                on_close_aside.call(());
                            },
                            Icon { glyph: "close-outline".to_string() }
                        }
                    }
                    div { class: "rightbar__body",
                        if let Some(chat) = aside.clone() {
                            SubagentAside { chat, on_close: on_close_aside }
                        }
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
