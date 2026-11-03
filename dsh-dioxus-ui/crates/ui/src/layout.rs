//! 三栏骨架：Sidebar / Main / RightBar + 设置浮层 + 状态栏。

use std::collections::HashMap;

use dioxus::prelude::*;

use crate::composer::{CommandItemView, FileRefItemView};
use crate::conversation::ConversationView;
use crate::file_tree::FileTree;
use crate::i18n::{self, Key, Locale, use_t};
use crate::icons::Icon;
use crate::model::{
    ApprovalItem, AsideChat, ConnectionState, FileEntry, MessageRow, PlanState, QuestionCard,
    QueueItem, SearchHit, SendMode, SendRequest, SessionSummary, SubagentMode, SubagentRow, Theme,
    TrajectoryRow, WorkspaceTab,
};
use crate::settings::{
    AgentPresetItemView, ModelGroupView, ModelItemView, PermissionItemView, SettingsChange,
    SettingsPanel, SettingsSnapshot,
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
    #[props(default = HashMap::new())] file_children: HashMap<String, Vec<FileEntry>>,
    #[props(default = Vec::new())] file_expanded: Vec<String>,
    #[props(default = false)] file_loading: bool,
    #[props(default = EventHandler::new(|_| {}))] on_file_toggle: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_file_refresh: EventHandler<()>,
    #[props(default = None)] preview: Option<String>,
    #[props(default = 0)] generation: u64,
    #[props(default = String::new())] host_label: String,
    #[props(default = String::new())] active_title: String,
    #[props(default = false)] running: bool,
    #[props(default = SendMode::Queue)] send_mode: SendMode,
    #[props(default = Theme::System)] theme: Theme,
    #[props(default = EventHandler::new(|_| {}))] on_theme: EventHandler<Theme>,
    #[props(default = EventHandler::new(|_| {}))] on_locale: EventHandler<Locale>,
    #[props(default = None)] settings: Option<SettingsSnapshot>,
    #[props(default = Vec::new())] model_groups: Vec<ModelGroupView>,
    #[props(default = Vec::new())] permissions: Vec<PermissionItemView>,
    #[props(default = Vec::new())] agent_presets: Vec<AgentPresetItemView>,
    #[props(default = None)] selected_model: Option<(String, String)>,
    #[props(default = None)] selected_permission: Option<String>,
    #[props(default = false)] settings_saving: bool,
    #[props(default = EventHandler::new(|_| {}))] on_settings_change: EventHandler<SettingsChange>,
    #[props(default = EventHandler::new(|_| {}))] on_select_model: EventHandler<(String, String)>,
    #[props(default = EventHandler::new(|_| {}))] on_select_permission: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_apply_preset: EventHandler<String>,
    #[props(default = Vec::new())] commands: Vec<CommandItemView>,
    #[props(default = Vec::new())] file_refs: Vec<FileRefItemView>,
    #[props(default = PlanState::default())] plan: PlanState,
    #[props(default = EventHandler::new(|_| {}))] on_command_query: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_file_query: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_copy: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_feedback: EventHandler<(String, bool)>,
    #[props(default = EventHandler::new(|_| {}))] on_retry: EventHandler<String>,
    #[props(default = Vec::new())] workspaces: Vec<WorkspaceTab>,
    #[props(default = Vec::new())] pinned: Vec<String>,
    #[props(default = String::new())] search_query: String,
    #[props(default = Vec::new())] search_results: Vec<SearchHit>,
    #[props(default = false)] searching: bool,
    #[props(default = EventHandler::new(|_| {}))] on_search: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_search_clear: EventHandler<()>,
    #[props(default = EventHandler::new(|_| {}))] on_pin: EventHandler<(String, bool)>,
    #[props(default = EventHandler::new(|_| {}))] on_fork: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_new_workspace: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_rename_workspace: EventHandler<(
        String,
        String,
    )>,
    #[props(default = EventHandler::new(|_| {}))] on_delete_workspace: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_move_workspace: EventHandler<(String, bool)>,
    #[props(default = None)] error: Option<String>,
    #[props(default = EventHandler::new(|_| {}))] on_dismiss_error: EventHandler<()>,
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
    #[props(default = EventHandler::new(|_| {}))] on_new_session: EventHandler<Option<String>>,
    on_send: EventHandler<SendRequest>,
    #[props(default = false)] is_subagent: bool,
    #[props(default = None)] subagent_mode: Option<SubagentMode>,
    #[props(default = true)] parent_available: bool,
    #[props(default = Vec::new())] lineage: Vec<(String, String)>,
    #[props(default = EventHandler::new(|_| {}))] on_navigate: EventHandler<String>,
    #[props(default = Vec::new())] subagent_rows: Vec<SubagentRow>,
    #[props(default = HashMap::new())] subagent_tree: HashMap<String, Vec<SubagentRow>>,
    #[props(default = Vec::new())] subagent_expanded: Vec<String>,
    #[props(default = None)] subagent_error: Option<String>,
    #[props(default = false)] subagent_loading: bool,
    #[props(default = false)] subagent_migration: bool,
    #[props(default = EventHandler::new(|_| {}))] on_subagent_toggle: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_subagent_open: EventHandler<SubagentRow>,
    #[props(default = EventHandler::new(|_| {}))] on_subagent_aside: EventHandler<SubagentRow>,
    #[props(default = EventHandler::new(|_| {}))] on_subagent_refresh: EventHandler<()>,
    #[props(default = None)] aside: Option<AsideChat>,
    #[props(default = EventHandler::new(|_| {}))] on_close_aside: EventHandler<()>,
) -> Element {
    let t = use_t();
    let debug = cfg!(debug_assertions);
    let mut collapsed = use_signal(|| false);
    let mut show_right =
        use_signal(|| debug && std::env::var("DSH_DEBUG_RIGHTBAR").ok().as_deref() == Some("1"));
    let mut show_settings =
        use_signal(|| debug && std::env::var("DSH_DEBUG_SETTINGS").ok().as_deref() == Some("1"));
    let mut sidebar_width = use_signal(|| 280.0f64);
    let mut dragging_sidebar = use_signal(|| false);
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
    let shell_style = if collapsed() {
        String::new()
    } else {
        format!("--shell-sidebar: {}px", sidebar_width())
    };
    let sidebar_style = if collapsed() {
        String::new()
    } else {
        format!("width: {}px", sidebar_width())
    };

    rsx! {
        div {
            class: "{shell_class}",
            style: "{shell_style}",
            onmousemove: move |evt| {
                if dragging_sidebar() {
                    sidebar_width.set(evt.client_coordinates().x.clamp(200.0, 420.0));
                }
            },
            onmouseup: move |_| dragging_sidebar.set(false),
            onmouseleave: move |_| dragging_sidebar.set(false),
            aside { class: "sidebar", style: "{sidebar_style}",
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
                        workspaces: workspaces.clone(),
                        pinned: pinned.clone(),
                        search_query: search_query.clone(),
                        search_results: search_results.clone(),
                        searching,
                        on_select,
                        on_new: on_new_session,
                        on_search,
                        on_search_clear,
                        on_pin,
                        on_fork,
                        on_new_workspace,
                        on_rename_workspace,
                        on_delete_workspace,
                        on_move_workspace,
                    }
                }
            }
            div {
                class: "sidebar-resizer",
                title: t.tr("拖动调整宽度", "Drag to resize"),
                onmousedown: move |_| dragging_sidebar.set(true),
                ondoubleclick: move |_| sidebar_width.set(280.0),
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
                    commands: commands.clone(),
                    file_refs: file_refs.clone(),
                    models: model_groups.iter().flat_map(|group| group.models.clone()).collect::<Vec<ModelItemView>>(),
                    selected_model: selected_model.clone(),
                    permissions: permissions.clone(),
                    selected_permission: selected_permission.clone(),
                    plan,
                    on_command_query,
                    on_file_query,
                    on_model: on_select_model,
                    on_permission: on_select_permission,
                    on_copy,
                    on_feedback,
                    on_retry,
                    on_file_ref: on_open_file,
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
                    subagent_error: subagent_error.clone(),
                    subagent_loading,
                    subagent_migration,
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
                        FileTree {
                            roots: files.clone(),
                            dir_children: file_children.clone(),
                            expanded: file_expanded.clone(),
                            loading: file_loading,
                            on_toggle: on_file_toggle,
                            on_open: on_open_file,
                            on_refresh: on_file_refresh,
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
            SettingsPanel {
                snapshot: settings.clone(),
                model_groups: model_groups.clone(),
                permissions: permissions.clone(),
                agent_presets: agent_presets.clone(),
                selected_model: selected_model.clone(),
                selected_permission: selected_permission.clone(),
                host_label: host_label.clone(),
                generation,
                saving: settings_saving,
                on_change: on_settings_change,
                on_select_model,
                on_select_permission,
                on_apply_preset,
                on_close: move |_| show_settings.set(false),
            }
        }

        if let Some(message) = error.clone() {
            div { class: "error-toast",
                span { class: "error-toast__text", "{message}" }
                button {
                    class: "icon-btn",
                    onclick: move |_| on_dismiss_error.call(()),
                    Icon { glyph: "close-outline".to_string(), size: 12.0 }
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
                title: "{t.get(Key::Reconnect)}",
                onclick: move |_| on_reconnect.call(()),
                Icon { glyph: "refresh-outline".to_string() }
            }
            button {
                class: "icon-btn",
                title: "{t.get(Key::Settings)}",
                onclick: move |_| show_settings.set(true),
                Icon { glyph: "settings-outline".to_string() }
            }
        }
    }
}
