//! 右栏文件树：懒加载目录、搜索、右键菜单。
//!
//! 数据由 App 维护（`children` 按目录 path 缓存）；组件只发展开/打开意图。

use std::collections::HashMap;

use dioxus::prelude::*;

use crate::i18n::use_t;
use crate::icons::Icon;
use crate::model::FileEntry;

#[component]
pub fn FileTree(
    roots: Vec<FileEntry>,
    #[props(default = HashMap::new())] dir_children: HashMap<String, Vec<FileEntry>>,
    #[props(default = Vec::new())] expanded: Vec<String>,
    #[props(default = false)] loading: bool,
    on_toggle: EventHandler<String>,
    on_open: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_refresh: EventHandler<()>,
) -> Element {
    let t = use_t();
    let mut query = use_signal(String::new);
    let mut menu = use_signal(|| None::<String>);

    let filter = query().trim().to_ascii_lowercase();
    let flat_matches: Vec<FileEntry> = if filter.is_empty() {
        Vec::new()
    } else {
        let mut all = roots.clone();
        for items in dir_children.values() {
            all.extend(items.iter().cloned());
        }
        all.into_iter()
            .filter(|item| item.path.to_ascii_lowercase().contains(&filter))
            .collect()
    };
    let has_matches = !flat_matches.is_empty();

    rsx! {
        div { class: "file-tree",
            div { class: "file-tree__bar",
                input {
                    class: "settings-input",
                    value: "{query}",
                    placeholder: t.tr("搜索文件", "Search files"),
                    oninput: move |evt| query.set(evt.value()),
                }
                button {
                    class: "icon-btn",
                    title: t.tr("刷新", "Refresh"),
                    onclick: move |_| on_refresh.call(()),
                    Icon { glyph: "refresh-outline".to_string(), size: 12.0 }
                }
            }
            if loading {
                div { class: "muted file-tree__empty", "…" }
            }
            if !filter.is_empty() {
                for item in flat_matches {
                    {
                        let path = item.path.clone();
                        rsx! {
                            button {
                                key: "{item.path}",
                                class: "file-row",
                                onclick: move |_| on_open.call(path.clone()),
                                span { class: "file-icon", Icon { glyph: "search-outline".to_string(), size: 11.0 } }
                                span { class: "file-name", "{item.path}" }
                            }
                        }
                    }
                }
                if !has_matches {
                    div { class: "muted file-tree__empty", {t.tr("没有匹配文件", "No matching files")} }
                }
            } else {
                for entry in roots.iter() {
                    FileNode {
                        entry: entry.clone(),
                        depth: 0,
                        dir_children: dir_children.clone(),
                        expanded: expanded.clone(),
                        menu: menu,
                        on_toggle: on_toggle,
                        on_open: on_open,
                    }
                }
                if roots.is_empty() && !loading {
                    div { class: "muted file-tree__empty", {t.tr("工作区为空", "Workspace is empty")} }
                }
            }
            if let Some(path) = menu() {
                div { class: "file-menu",
                    button {
                        class: "sidebar-menu__item",
                        onclick: {
                            let path = path.clone();
                            move |_| {
                                on_open.call(path.clone());
                                menu.set(None);
                            }
                        },
                        {t.tr("打开预览", "Open preview")}
                    }
                    button {
                        class: "sidebar-menu__item",
                        onclick: {
                            let path = path.clone();
                            move |_| {
                                let escaped = serde_json::to_string(&path)
                                    .unwrap_or_else(|_| "\"\"".to_string());
                                let _ = document::eval(&format!(
                                    "navigator.clipboard.writeText({escaped})"
                                ));
                                menu.set(None);
                            }
                        },
                        {t.tr("复制路径", "Copy path")}
                    }
                }
            }
        }
    }
}

#[component]
fn FileNode(
    entry: FileEntry,
    depth: usize,
    dir_children: HashMap<String, Vec<FileEntry>>,
    expanded: Vec<String>,
    menu: Signal<Option<String>>,
    on_toggle: EventHandler<String>,
    on_open: EventHandler<String>,
) -> Element {
    let open = expanded.iter().any(|path| path == &entry.path);
    let node_children = dir_children.get(&entry.path).cloned().unwrap_or_default();
    let path = entry.path.clone();
    let open_path = entry.path.clone();
    let context_path = entry.path.clone();

    rsx! {
        div { class: "file-node",
            button {
                key: "{entry.path}",
                class: "file-row",
                style: "padding-left: {8 + depth * 14}px",
                onclick: move |_| {
                    if entry.is_dir {
                        on_toggle.call(path.clone());
                    } else {
                        on_open.call(open_path.clone());
                    }
                },
                oncontextmenu: move |evt| {
                    evt.prevent_default();
                    menu.set(Some(context_path.clone()));
                },
                span { class: "file-icon",
                    if entry.is_dir {
                        Icon {
                            glyph: if open { "chevron-down-outline" } else { "chevron-right-outline" },
                            size: 11.0
                        }
                    } else {
                        Icon { glyph: "chevron-right-outline".to_string(), size: 11.0 }
                    }
                }
                span { class: "file-name", "{entry.name}" }
                if entry.is_dir && !dir_children.contains_key(&entry.path) {
                    span { class: "muted file-node__hint", "…" }
                }
            }
            if entry.is_dir && open {
                for child in node_children.iter() {
                    FileNode {
                        entry: child.clone(),
                        depth: depth + 1,
                        dir_children: dir_children.clone(),
                        expanded: expanded.clone(),
                        menu: menu,
                        on_toggle: on_toggle,
                        on_open: on_open,
                    }
                }
            }
        }
    }
}
