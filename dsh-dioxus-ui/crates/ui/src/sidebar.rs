//! 左栏：搜索 + 置顶 + 按工作区分组的会话列表。
//!
//! 对应上游 `ui-sidebar` / `ui-workspace`：workspace 分组、会话项、状态点、
//! 会话菜单（置顶 / fork）与工作区菜单（重命名 / 删除 / 新建会话）。

use std::collections::HashSet;

use dioxus::prelude::*;

use crate::i18n::{Key, use_t};
use crate::icons::Icon;
use crate::model::{SearchHit, SessionSummary, WorkspaceTab};

#[component]
pub fn SessionList(
    sessions: Vec<SessionSummary>,
    active_id: Option<String>,
    #[props(default = Vec::new())] workspaces: Vec<WorkspaceTab>,
    #[props(default = Vec::new())] pinned: Vec<String>,
    #[props(default = String::new())] search_query: String,
    #[props(default = Vec::new())] search_results: Vec<SearchHit>,
    #[props(default = false)] searching: bool,
    /// 右侧会话面板不再重复「新建工作区」（入口在左侧资源管理器）。
    #[props(default = true)]
    show_workspace_add: bool,
    on_select: EventHandler<String>,
    on_new: EventHandler<Option<String>>,
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
) -> Element {
    let t = use_t();
    let session_menu = use_signal(|| None::<String>);
    let mut group_menu = use_signal(|| None::<String>);
    let mut renaming = use_signal(|| None::<String>);
    let mut title_draft = use_signal(String::new);
    let mut adding_workspace = use_signal(|| false);
    let mut workspace_path = use_signal(String::new);

    let pinned_set: HashSet<&str> = pinned.iter().map(String::as_str).collect();
    let mut groups: Vec<(String, Vec<SessionSummary>)> = Vec::new();
    for session in sessions {
        match groups
            .iter_mut()
            .find(|(name, _)| *name == session.workspace)
        {
            Some((_, items)) => items.push(session),
            None => groups.push((session.workspace.clone(), vec![session])),
        }
    }

    let mut pinned_sessions: Vec<SessionSummary> = groups
        .iter()
        .flat_map(|(_, items)| items.iter())
        .filter(|session| pinned_set.contains(session.id.as_str()))
        .cloned()
        .collect();
    pinned_sessions.sort_by_key(|session| {
        pinned
            .iter()
            .position(|id| id == &session.id)
            .unwrap_or(usize::MAX)
    });

    rsx! {
        nav { class: "session-list",
            div { class: "sidebar-search",
                Icon { glyph: "search-outline".to_string(), size: 13.0 }
                input {
                    class: "sidebar-search__input",
                    value: "{search_query}",
                    placeholder: t.tr("搜索会话", "Search sessions"),
                    oninput: move |evt| on_search.call(evt.value()),
                }
                if !search_query.is_empty() {
                    button {
                        class: "icon-btn",
                        title: t.tr("清除", "Clear"),
                        onclick: move |_| on_search_clear.call(()),
                        Icon { glyph: "close-outline".to_string(), size: 12.0 }
                    }
                }
            }

            button {
                class: "primary-btn primary-btn--block",
                onclick: move |_| on_new.call(None),
                Icon { glyph: "new-chat-outline".to_string() }
                span { "{t.get(Key::NewSession)}" }
            }
            if show_workspace_add {
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

            if !search_query.is_empty() {
                div { class: "group",
                    div { class: "group__title",
                        "{t.tr(\"搜索结果\", \"Search results\")} · {search_results.len()}"
                        if searching {
                            span { class: "chip chip--live", "…" }
                        }
                    }
                    for hit in search_results.iter() {
                        button {
                            key: "{hit.session_id}",
                            class: "search-hit",
                            onclick: {
                                let id = hit.session_id.clone();
                                move |_| on_select.call(id.clone())
                            },
                            span { class: "search-hit__snippet", "{hit.snippet}" }
                        }
                    }
                    if search_results.is_empty() && !searching {
                        div { class: "muted sidebar-empty", "{t.tr(\"没有匹配会话\", \"No matching sessions\")}" }
                    }
                }
            } else {
                if !pinned_sessions.is_empty() {
                    div { class: "group",
                        div { class: "group__title", "{t.tr(\"置顶\", \"Pinned\")}" }
                        for session in pinned_sessions {
                            {session_row(t, session, active_id.clone(), true, session_menu, on_select, on_pin, on_fork)}
                        }
                    }
                }

                for (workspace, items) in groups {
                    {
                        let workspace_meta = workspaces.iter().find(|item| item.title == workspace).cloned();
                        let workspace_id = workspace_meta.as_ref().map(|item| item.id.clone());
                        let workspace_pos = workspace_id
                            .as_ref()
                            .and_then(|id| workspaces.iter().position(|item| &item.id == id));
                        let items: Vec<SessionSummary> = items
                            .into_iter()
                            .filter(|session| !pinned_set.contains(session.id.as_str()))
                            .collect();
                        let can_move_up = workspace_pos.is_some_and(|pos| pos > 0);
                        let can_move_down = workspace_pos
                            .is_some_and(|pos| pos + 1 < workspaces.len());
                        let group_open = group_menu().as_deref() == Some(workspace.as_str());
                        let group_renaming = renaming().as_deref() == Some(workspace.as_str());
                        let title = workspace.clone();
                        let title_for_input = workspace.clone();
                        let workspace_for_new = workspace_id.clone();
                        rsx! {
                            div { key: "{workspace}", class: "group",
                                div { class: "group__head",
                                    if group_renaming {
                                        input {
                                            class: "title-input",
                                            value: "{title_draft}",
                                            autofocus: true,
                                            oninput: move |evt| title_draft.set(evt.value()),
                                            onkeydown: {
                                                let id = workspace_id.clone();
                                                move |evt| match evt.key().to_string().as_str() {
                                                    "Enter" => {
                                                        if let (Some(id), value) = (id.clone(), title_draft())
                                                            && !value.trim().is_empty()
                                                        {
                                                            on_rename_workspace.call((id, value));
                                                        }
                                                        renaming.set(None);
                                                    }
                                                    "Escape" => renaming.set(None),
                                                    _ => {}
                                                }
                                            },
                                        }
                                    } else {
                                        div {
                                            class: "group__title",
                                            title: workspace_meta.as_ref().map(|item| item.path.clone()).unwrap_or_default(),
                                            "{title}"
                                        }
                                    }
                                    span { class: "spacer" }
                                    button {
                                        class: "icon-btn",
                                        title: t.tr("在此工作区新建会话", "New session in workspace"),
                                        onclick: move |_| on_new.call(workspace_for_new.clone()),
                                        Icon { glyph: "plus-outline".to_string(), size: 12.0 }
                                    }
                                    button {
                                        class: "icon-btn",
                                        title: t.tr("工作区菜单", "Workspace menu"),
                                        onclick: move |_| {
                                            if group_open { group_menu.set(None); }
                                            else { group_menu.set(Some(workspace.clone())); }
                                        },
                                        Icon { glyph: "ellipsis-outline".to_string(), size: 13.0 }
                                    }
                                }
                                if group_open {
                                    div { class: "sidebar-menu",
                                        button {
                                            class: "sidebar-menu__item",
                                            onclick: {
                                                let title = title_for_input.clone();
                                                move |_| {
                                                    title_draft.set(title.clone());
                                                    renaming.set(Some(title.clone()));
                                                    group_menu.set(None);
                                                }
                                            },
                                            "{t.tr(\"重命名工作区\", \"Rename workspace\")}"
                                        }
                                        button {
                                            class: "sidebar-menu__item",
                                            disabled: !can_move_up,
                                            onclick: {
                                                let id = workspace_id.clone();
                                                move |_| {
                                                    if let Some(id) = id.clone() {
                                                        on_move_workspace.call((id, true));
                                                    }
                                                    group_menu.set(None);
                                                }
                                            },
                                            "{t.tr(\"上移工作区\", \"Move workspace up\")}"
                                        }
                                        button {
                                            class: "sidebar-menu__item",
                                            disabled: !can_move_down,
                                            onclick: {
                                                let id = workspace_id.clone();
                                                move |_| {
                                                    if let Some(id) = id.clone() {
                                                        on_move_workspace.call((id, false));
                                                    }
                                                    group_menu.set(None);
                                                }
                                            },
                                            "{t.tr(\"下移工作区\", \"Move workspace down\")}"
                                        }
                                        button {
                                            class: "sidebar-menu__item sidebar-menu__item--danger",
                                            disabled: workspace_id.is_none(),
                                            onclick: {
                                                let id = workspace_id.clone();
                                                move |_| {
                                                    if let Some(id) = id.clone() {
                                                        on_delete_workspace.call(id);
                                                    }
                                                    group_menu.set(None);
                                                }
                                            },
                                            "{t.tr(\"删除工作区\", \"Delete workspace\")}"
                                        }
                                    }
                                }
                                for session in items {
                                    {session_row(t, session, active_id.clone(), false, session_menu, on_select, on_pin, on_fork)}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn session_row(
    t: crate::i18n::T,
    session: SessionSummary,
    active_id: Option<String>,
    pinned: bool,
    mut menu: Signal<Option<String>>,
    on_select: EventHandler<String>,
    on_pin: EventHandler<(String, bool)>,
    on_fork: EventHandler<String>,
) -> Element {
    let id = session.id.clone();
    let selected = active_id.as_deref() == Some(session.id.as_str());
    let class = if selected {
        "session-item session-item--active"
    } else {
        "session-item"
    };
    let menu_open = menu().as_deref() == Some(session.id.as_str());
    let pin_id = id.clone();
    let fork_id = id.clone();
    let select_id = id.clone();
    let menu_id = id.clone();

    rsx! {
        div { key: "{session.id}", class: "session-row",
            button {
                class: "{class}",
                onclick: move |_| on_select.call(select_id.clone()),
                span { class: "{session.status.css_class()}" }
                span { class: "session-item__title", "{session.title}" }
                span { class: "session-item__time muted", "{session.updated}" }
            }
            button {
                class: "icon-btn session-row__menu",
                title: t.tr("会话菜单", "Session menu"),
                onclick: move |_| {
                    if menu_open { menu.set(None); }
                    else { menu.set(Some(menu_id.clone())); }
                },
                Icon { glyph: "ellipsis-outline".to_string(), size: 13.0 }
            }
            if menu_open {
                div { class: "sidebar-menu sidebar-menu--row",
                    button {
                        class: "sidebar-menu__item",
                        onclick: move |_| {
                            on_pin.call((pin_id.clone(), !pinned));
                            menu.set(None);
                        },
                        {if pinned { t.tr("取消置顶", "Unpin") } else { t.tr("置顶", "Pin") }}
                    }
                    button {
                        class: "sidebar-menu__item",
                        onclick: move |_| {
                            on_fork.call(fork_id.clone());
                            menu.set(None);
                        },
                        "{t.tr(\"Fork 会话\", \"Fork session\")}"
                    }
                }
            }
        }
    }
}
