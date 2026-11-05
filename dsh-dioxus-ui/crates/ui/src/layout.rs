//! CodeBuddy 式工作台骨架：
//!
//! ```text
//! ┌────┬──────────────┬────────────────────────────┬────────────┐
//! │活动│ 资源管理器   │ 编辑区 tabs：对话/预览/…   │ 会话面板   │
//! │栏  │ 文件树       │────────────────────────────│ 列表/搜索  │
//! │    │ +新建工作区  │ 底部：输入/终端/任务/计划  │ 新建会话   │
//! └────┴──────────────┴────────────────────────────┴────────────┘
//!                        状态栏
//! ```
//!
//! 与 CodeBuddy 的对应关系：
//! - 活动栏 + 资源管理器：左侧文件树与工作区入口；
//! - 编辑区：会话以 tab 形式承载，文件预览与子会话副本追加为 tab；
//! - 底部面板：Composer（输入）与终端/后台任务/计划共用；
//! - 右侧面板：会话管理（搜索、新建、工作区、置顶/fork）。

use std::collections::HashMap;

use dioxus::prelude::*;

use crate::composer::{CommandItemView, Composer, FileRefItemView};
use crate::conversation::ConversationView;
use crate::editor::EditorPane;
use crate::file_tree::FileTree;
use crate::i18n::{self, Key, Locale, use_t};
use crate::icons::Icon;
use crate::markdown::MarkdownView;
use crate::model::{
    ApprovalItem, AsideChat, ConnectionState, FileEntry, JobRow, MessageRow, PlanState, PreviewDoc,
    QuestionCard, QueueItem, SearchHit, SendMode, SendRequest, SessionSummary, SubagentMode,
    SubagentRow, TerminalView, Theme, TrajectoryRow, WorkspaceTab,
};
use crate::settings::{
    AgentPresetItemView, ModelGroupView, ModelItemView, PermissionItemView, ProviderRow,
    SettingsChange, SettingsPanel, SettingsSnapshot,
};
use crate::sidebar::SessionList;
use crate::terminal::TerminalPanel;

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
    #[props(default = None)] preview: Option<PreviewDoc>,
    #[props(default = EventHandler::new(|_| {}))] on_close_preview: EventHandler<()>,
    /// 编辑区当前 tab：0 = 对话，1 = 预览，2 = 子会话。
    #[props(default = 0u8)]
    editor_tab: u8,
    #[props(default = EventHandler::new(|_| {}))] on_editor_tab: EventHandler<u8>,
    #[props(default = Vec::new())] jobs: Vec<JobRow>,
    #[props(default = None)] plan_markdown: Option<String>,
    #[props(default = None)] terminal: Option<TerminalView>,
    #[props(default = false)] terminal_loading: bool,
    #[props(default = EventHandler::new(|_| {}))] on_terminal_open: EventHandler<()>,
    #[props(default = EventHandler::new(|_| {}))] on_terminal_close: EventHandler<()>,
    #[props(default = EventHandler::new(|_| {}))] on_terminal_input: EventHandler<String>,
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
    #[props(default = Vec::new())] providers: Vec<ProviderRow>,
    #[props(default = Vec::new())] permissions: Vec<PermissionItemView>,
    #[props(default = Vec::new())] agent_presets: Vec<AgentPresetItemView>,
    #[props(default = None)] selected_model: Option<(String, String)>,
    #[props(default = None)] selected_permission: Option<String>,
    #[props(default = false)] settings_saving: bool,
    #[props(default = EventHandler::new(|_| {}))] on_settings_change: EventHandler<SettingsChange>,
    #[props(default = EventHandler::new(|_| {}))] on_select_model: EventHandler<(String, String)>,
    #[props(default = EventHandler::new(|_| {}))] on_provider_save: EventHandler<(String, String)>,
    #[props(default = EventHandler::new(|_| {}))] on_provider_remove: EventHandler<String>,
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
    // 调试钩子：DSH_DEBUG_TERMINAL=1 时启动即切到终端 tab
    let debug_terminal = debug && std::env::var("DSH_DEBUG_TERMINAL").ok().as_deref() == Some("1");
    let mut show_settings =
        use_signal(|| debug && std::env::var("DSH_DEBUG_SETTINGS").ok().as_deref() == Some("1"));

    // 左侧资源管理器（只有文件树）
    let mut sidebar_open = use_signal(|| true);
    let mut sidebar_width = use_signal(|| 260.0f64);
    let mut dragging_sidebar = use_signal(|| false);
    let mut sidebar_drag = use_signal(|| (0.0f64, 0.0f64));
    let mut adding_workspace = use_signal(|| false);
    let mut workspace_path = use_signal(String::new);

    // 中间底部面板：输入 / 终端 / 任务 / 计划
    let mut bottom_open = use_signal(|| true);
    // 调试钩子：DSH_DEBUG_BOTTOM_TAB=0..3 指定初始 tab（视觉验收用）
    let initial_bottom_tab = std::env::var("DSH_DEBUG_BOTTOM_TAB")
        .ok()
        .and_then(|value| value.parse::<u8>().ok())
        .filter(|value| *value <= 3)
        .unwrap_or(if debug_terminal { 1 } else { 0 });
    let mut bottom_tab = use_signal(move || initial_bottom_tab);
    let mut bottom_height = use_signal(|| 210.0f64);
    let mut dragging_bottom = use_signal(|| false);
    let mut bottom_drag = use_signal(|| (0.0f64, 0.0f64));

    // 右侧会话管理
    let mut session_open = use_signal(|| true);
    let mut session_width = use_signal(|| 300.0f64);
    let mut dragging_session = use_signal(|| false);
    let mut session_drag = use_signal(|| (0.0f64, 0.0f64));

    let sidebar_style = format!("width: {}px", sidebar_width());
    let session_style = format!("width: {}px", session_width());
    let bottom_style = format!("height: {}px", bottom_height());

    // Composer 与 ConversationView 共用
    let can_send = connection == ConnectionState::Connected;
    let read_only_reason = match subagent_mode {
        Some(SubagentMode::Continuable) if !parent_available => {
            Some(Key::SubagentParentUnavailable)
        }
        Some(SubagentMode::OneShot) => Some(Key::SubagentReadOnlyOneShot),
        Some(SubagentMode::External) => Some(Key::SubagentReadOnlyExternal),
        Some(SubagentMode::Unknown) => Some(Key::SubagentReadOnlyUnknown),
        _ => None,
    };
    let composer_notice = read_only_reason.map(|key| t.get(key).to_string());
    let can_stop = subagent_mode
        .map(SubagentMode::is_continuable)
        .unwrap_or(false);
    let model_items = model_groups
        .iter()
        .flat_map(|group| group.models.clone())
        .collect::<Vec<ModelItemView>>();
    let conversation_title = if active_title.is_empty() {
        t.tr("对话", "Chat").to_string()
    } else {
        active_title.clone()
    };

    rsx! {
        div {
            class: "workbench",
            onmousemove: move |evt| {
                let (x, y) = (evt.client_coordinates().x, evt.client_coordinates().y);
                if dragging_sidebar() {
                    let (start_x, start_width) = sidebar_drag();
                    sidebar_width.set((start_width + x - start_x).clamp(180.0, 420.0));
                }
                if dragging_session() {
                    let (start_x, start_width) = session_drag();
                    session_width.set((start_width + start_x - x).clamp(240.0, 460.0));
                }
                if dragging_bottom() {
                    let (start_y, start_height) = bottom_drag();
                    bottom_height.set((start_height + start_y - y).clamp(140.0, 560.0));
                }
            },
            onmouseup: move |_| {
                dragging_sidebar.set(false);
                dragging_session.set(false);
                dragging_bottom.set(false);
            },
            onmouseleave: move |_| {
                dragging_sidebar.set(false);
                dragging_session.set(false);
                dragging_bottom.set(false);
            },

            div { class: "workbench__body",
                // ---------------------------------------------------- 活动栏
                nav { class: "activitybar",
                    div { class: "activitybar__brand", "d" }
                    button {
                        class: if sidebar_open() { "activitybar__item activitybar__item--active" } else { "activitybar__item" },
                        title: t.tr("资源管理器", "Explorer"),
                        onclick: move |_| sidebar_open.toggle(),
                        Icon { glyph: "folder-outline".to_string(), size: 20.0 }
                    }
                    span { class: "spacer" }
                    button {
                        class: "activitybar__item",
                        title: t.get(Key::Settings),
                        onclick: move |_| show_settings.set(true),
                        Icon { glyph: "settings-outline".to_string(), size: 20.0 }
                    }
                }

                // ---------------------------------------------- 资源管理器
                if sidebar_open() {
                    aside { class: "explorer", style: "{sidebar_style}",
                        div { class: "explorer__head",
                            span { class: "explorer__title", "{t.tr(\"资源管理器\", \"Explorer\")}" }
                            span { class: "spacer" }
                            button {
                                class: "icon-btn",
                                title: t.tr("刷新", "Refresh"),
                                onclick: move |_| on_file_refresh.call(()),
                                Icon { glyph: "refresh-outline".to_string(), size: 14.0 }
                            }
                            button {
                                class: "icon-btn",
                                title: t.tr("收起侧栏", "Hide sidebar"),
                                onclick: move |_| sidebar_open.set(false),
                                Icon { glyph: "panel-left-outline".to_string(), size: 14.0 }
                            }
                        }
                        div { class: "explorer__body",
                            div { class: "explorer__new-workspace",
                                button {
                                    class: "ghost-btn sidebar-workspace-add",
                                    onclick: move |_| adding_workspace.toggle(),
                                    Icon { glyph: "plus-outline".to_string(), size: 12.0 }
                                    span { "{t.tr(\"新建工作区\", \"New workspace\")}" }
                                }
                                if adding_workspace() {
                                    div { class: "sidebar-inline-form",
                                        input {
                                            class: "settings-input",
                                            value: "{workspace_path}",
                                            placeholder: t.tr("工作区目录路径", "Workspace directory path"),
                                            oninput: move |evt| workspace_path.set(evt.value()),
                                            onkeydown: move |evt| {
                                                if evt.key().to_string() == "Enter" {
                                                    let path = workspace_path().trim().to_string();
                                                    if !path.is_empty() {
                                                        on_new_workspace.call(path);
                                                        workspace_path.set(String::new());
                                                        adding_workspace.set(false);
                                                    }
                                                }
                                            },
                                        }
                                        div { class: "sidebar-inline-actions",
                                            button {
                                                class: "ghost-btn",
                                                onclick: move |_| {
                                                    let path = workspace_path().trim().to_string();
                                                    if !path.is_empty() {
                                                        on_new_workspace.call(path);
                                                        workspace_path.set(String::new());
                                                        adding_workspace.set(false);
                                                    }
                                                },
                                                "{t.tr(\"创建\", \"Create\")}"
                                            }
                                            button {
                                                class: "ghost-btn",
                                                onclick: move |_| adding_workspace.set(false),
                                                "{t.tr(\"取消\", \"Cancel\")}"
                                            }
                                        }
                                    }
                                }
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
                        }
                    }
                    div {
                        class: "explorer-resizer",
                        title: t.tr("拖动调整宽度", "Drag to resize"),
                        onmousedown: move |evt| {
                            sidebar_drag.set((evt.client_coordinates().x, sidebar_width()));
                            dragging_sidebar.set(true);
                        },
                        ondoubleclick: move |_| sidebar_width.set(260.0),
                    }
                }

                // ------------------------------------------------ 中间列
                div { class: "center",
                    EditorPane {
                        tab: editor_tab,
                        on_tab: on_editor_tab,
                        preview: preview.clone(),
                        aside: aside.clone(),
                        session_title: conversation_title.clone(),
                        on_copy,
                        on_close_preview,
                        on_close_aside,

                        ConversationView {
                            active_id: active_id.clone(),
                            title: conversation_title.clone(),
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
                            models: model_items.clone(),
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
                            on_toggle_right: move |_| session_open.toggle(),
                            show_composer: false,
                        }
                    }

                    if bottom_open() {
                        div {
                            class: "bottom-resizer",
                            title: t.tr("拖动调整高度", "Drag to resize"),
                            onmousedown: move |evt| {
                                bottom_drag.set((evt.client_coordinates().y, bottom_height()));
                                dragging_bottom.set(true);
                            },
                            ondoubleclick: move |_| bottom_height.set(210.0),
                        }
                        section { class: "bottom-panel", style: "{bottom_style}",
                            div { class: "bottom-panel__head",
                                div { class: "tabs",
                                    button {
                                        class: if bottom_tab() == 0 { "tab tab--active" } else { "tab" },
                                        onclick: move |_| bottom_tab.set(0),
                                        "{t.tr(\"输入\", \"Input\")}"
                                    }
                                    button {
                                        class: if bottom_tab() == 1 { "tab tab--active" } else { "tab" },
                                        onclick: move |_| {
                                            bottom_tab.set(1);
                                            let _ = document::eval(
                                                "setTimeout(() => document.querySelector('.terminal__field')?.focus(), 30)",
                                            );
                                        },
                                        "{t.tr(\"终端\", \"Terminal\")}"
                                    }
                                    button {
                                        class: if bottom_tab() == 2 { "tab tab--active" } else { "tab" },
                                        onclick: move |_| bottom_tab.set(2),
                                        "{t.tr(\"任务\", \"Jobs\")} {jobs.len()}"
                                    }
                                    button {
                                        class: if bottom_tab() == 3 { "tab tab--active" } else { "tab" },
                                        onclick: move |_| bottom_tab.set(3),
                                        "{t.tr(\"计划\", \"Plan\")}"
                                    }
                                }
                                span { class: "spacer" }
                                button {
                                    class: "icon-btn",
                                    title: t.tr("收起面板", "Hide panel"),
                                    onclick: move |_| bottom_open.set(false),
                                    Icon { glyph: "close-outline".to_string(), size: 12.0 }
                                }
                            }
                            div { class: "bottom-panel__body",
                                // 四个 tab 常驻挂载、用 display 切换：切到终端再回来时
                                // Composer 草稿与终端屏幕都不会丢。
                                div {
                                    class: if bottom_tab() == 0 { "bottom-panel__tabpane" } else { "bottom-panel__tabpane is-hidden" },
                                    div { class: "bottom-panel__composer",
                                        Composer {
                                            can_send,
                                            running,
                                            send_mode,
                                            notice: composer_notice.clone(),
                                            can_stop,
                                            commands: commands.clone(),
                                            file_refs: file_refs.clone(),
                                            models: model_items.clone(),
                                            selected_model: selected_model.clone(),
                                            permissions: permissions.clone(),
                                            selected_permission: selected_permission.clone(),
                                            plan,
                                            on_send,
                                            on_mode,
                                            on_cancel,
                                            on_model: on_select_model,
                                            on_permission: on_select_permission,
                                            on_command_query,
                                            on_file_query,
                                        }
                                    }
                                }
                                div {
                                    class: if bottom_tab() == 1 { "bottom-panel__tabpane" } else { "bottom-panel__tabpane is-hidden" },
                                    TerminalPanel {
                                        terminal: terminal.clone(),
                                        loading: terminal_loading,
                                        on_open: on_terminal_open,
                                        on_close: on_terminal_close,
                                        on_input: on_terminal_input,
                                        on_copy,
                                    }
                                }
                                div {
                                    class: if bottom_tab() == 2 { "bottom-panel__tabpane" } else { "bottom-panel__tabpane is-hidden" },
                                    div { class: "bottom-panel__list",
                                        for job in jobs.iter() {
                                            div { key: "{job.id}", class: "job-row",
                                                span {
                                                    class: if job.status == "running" { "job-dot job-dot--run" } else { "job-dot" },
                                                }
                                                span { class: "job-row__label mono", "{job.label}" }
                                                span { class: "job-row__status muted", "{job.status}" }
                                            }
                                        }
                                        if jobs.is_empty() {
                                            div { class: "muted bottom-panel__empty", "{t.tr(\"暂无后台任务\", \"No background jobs\")}" }
                                        }
                                    }
                                }
                                div {
                                    class: if bottom_tab() == 3 { "bottom-panel__tabpane" } else { "bottom-panel__tabpane is-hidden" },
                                    div { class: "bottom-panel__md md",
                                        if let Some(plan) = plan_markdown.clone() {
                                            MarkdownView { source: plan }
                                        } else {
                                            div { class: "muted bottom-panel__empty", "{t.tr(\"当前没有计划\", \"No plan yet\")}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // ---------------------------------------------- 会话面板
                if session_open() {
                    div {
                        class: "session-resizer",
                        title: t.tr("拖动调整宽度", "Drag to resize"),
                        onmousedown: move |evt| {
                            session_drag.set((evt.client_coordinates().x, session_width()));
                            dragging_session.set(true);
                        },
                        ondoubleclick: move |_| session_width.set(300.0),
                    }
                    aside { class: "sessionpane", style: "{session_style}",
                        div { class: "sessionpane__head",
                            span { class: "explorer__title", "{t.tr(\"会话\", \"Sessions\")}" }
                            span { class: "spacer" }
                            button {
                                class: "icon-btn",
                                title: t.tr("新建会话", "New session"),
                                onclick: move |_| on_new_session.call(None),
                                Icon { glyph: "plus-outline".to_string(), size: 14.0 }
                            }
                            button {
                                class: "icon-btn",
                                title: t.tr("收起面板", "Hide panel"),
                                onclick: move |_| session_open.set(false),
                                Icon { glyph: "panel-left-outline".to_string(), size: 14.0 }
                            }
                        }
                        div { class: "sessionpane__body",
                            SessionList {
                                sessions: sessions.clone(),
                                active_id: active_id.clone(),
                                workspaces: workspaces.clone(),
                                pinned: pinned.clone(),
                                search_query: search_query.clone(),
                                search_results: search_results.clone(),
                                searching,
                                show_workspace_add: false,
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
                }
            }

            footer { class: "statusbar",
                span { class: "{connection.css_class()}", "{t.get(i18n::connection_key(connection))}" }
                span { class: "muted", "{host_label}" }
                span { class: "spacer" }
                button {
                    class: "statusbar__btn",
                    title: t.tr("会话面板", "Session panel"),
                    onclick: move |_| session_open.toggle(),
                    Icon { glyph: "new-chat-outline".to_string(), size: 13.0 }
                }
                button {
                    class: "statusbar__btn",
                    title: t.tr("底部面板", "Bottom panel"),
                    onclick: move |_| {
                        if bottom_open() && bottom_tab() == 1 {
                            bottom_open.set(false);
                        } else {
                            bottom_tab.set(1);
                            bottom_open.set(true);
                        }
                    },
                    Icon { glyph: "terminal-outline".to_string(), size: 13.0 }
                }
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

        if show_settings() {
            SettingsPanel {
                snapshot: settings.clone(),
                model_groups: model_groups.clone(),
                providers: providers.clone(),
                permissions: permissions.clone(),
                agent_presets: agent_presets.clone(),
                selected_model: selected_model.clone(),
                selected_permission: selected_permission.clone(),
                host_label: host_label.clone(),
                generation,
                saving: settings_saving,
                on_change: on_settings_change,
                on_select_model,
                on_provider_save,
                on_provider_remove,
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
    }
}
