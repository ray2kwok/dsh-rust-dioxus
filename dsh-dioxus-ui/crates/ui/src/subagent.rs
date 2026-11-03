//! 子 Agent 目录（集群视图）。
//!
//! 对应上游 `packages/client/ui-subagent/src/client/SubagentHeaderLineage.tsx`：
//! 会话 header 的后代计数触发器 + 可递归展开的目录树。
//!
//! 组件不取数据：目录、展开状态与回调都由 App 传入；组件只负责渲染与触发。

use std::collections::HashMap;

use dioxus::prelude::*;

use crate::i18n::{self, Key, use_t};
use crate::icons::Icon;
use crate::model::{AsideChat, SubagentMode, SubagentRow, SubagentState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatalogView {
    List,
    Graph,
}

fn view_button_class(active: bool) -> &'static str {
    if active { "seg seg--active" } else { "seg" }
}

#[component]
pub fn SubagentCatalog(
    rows: Vec<SubagentRow>,
    #[props(default = HashMap::new())] tree: HashMap<String, Vec<SubagentRow>>,
    #[props(default = Vec::new())] expanded: Vec<String>,
    #[props(default = String::new())] root_label: String,
    #[props(default = EventHandler::new(|_| {}))] on_toggle: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_open: EventHandler<SubagentRow>,
    #[props(default = EventHandler::new(|_| {}))] on_open_aside: EventHandler<SubagentRow>,
    #[props(default = EventHandler::new(|_| {}))] on_refresh: EventHandler<()>,
) -> Element {
    let t = use_t();
    // 调试开关：无头截图 / 手工验收时直接打开关系图，不影响默认行为。
    let debug_graph = cfg!(debug_assertions)
        && std::env::var("DSH_DEBUG_SUBAGENT_GRAPH").ok().as_deref() == Some("1");
    let mut open = use_signal(move || debug_graph);
    let mut view = use_signal(move || {
        if debug_graph {
            CatalogView::Graph
        } else {
            CatalogView::List
        }
    });

    if rows.is_empty() {
        return rsx! {};
    }

    let running = rows.iter().filter(|row| row.is_running()).count();
    let flat = flatten(&tree, &rows, &expanded);

    rsx! {
        div { class: "subagent-catalog",
            button {
                class: if open() { "subagent-trigger subagent-trigger--open" } else { "subagent-trigger" },
                title: "{t.get(Key::Subagents)}",
                onclick: move |_| {
                    let next = !open();
                    open.set(next);
                    if next {
                        on_refresh.call(());
                    }
                },
                Icon { glyph: "branch-outline".to_string(), size: 14.0 }
                span { "{t.get(Key::Subagents)}" }
                span { class: "subagent-trigger__count", "{rows.len()}" }
                if running > 0 {
                    span { class: "chip chip--live", "{running}" }
                }
            }
            button {
                class: "icon-btn subagent-graph-entry",
                title: "{t.get(Key::GraphView)}",
                onclick: move |_| {
                    open.set(true);
                    view.set(CatalogView::Graph);
                    on_refresh.call(());
                },
                Icon { glyph: "graph-outline".to_string(), size: 14.0 }
            }

            if open() {
                div {
                    class: if view() == CatalogView::Graph { "subagent-menu subagent-menu--graph" } else { "subagent-menu" },
                    div { class: "subagent-menu__head",
                        div { class: "segmented subagent-view-switch",
                            button {
                                class: "{view_button_class(view() == CatalogView::List)}",
                                onclick: move |_| view.set(CatalogView::List),
                                "{t.get(Key::ListView)}"
                            }
                            button {
                                class: "{view_button_class(view() == CatalogView::Graph)}",
                                onclick: move |_| view.set(CatalogView::Graph),
                                "{t.get(Key::GraphView)}"
                            }
                        }
                        span { class: "muted", "{rows.len()} · {t.get(Key::StatusRunning)} {running}" }
                        span { class: "spacer" }
                        button {
                            class: "icon-btn",
                            title: "{t.get(Key::Reconnect)}",
                            onclick: move |_| on_refresh.call(()),
                            Icon { glyph: "refresh-outline".to_string(), size: 14.0 }
                        }
                    }
                    if view() == CatalogView::List {
                        if flat.is_empty() {
                            div { class: "subagent-empty muted", "{t.get(Key::SubagentEmpty)}" }
                        }
                        for (depth, row) in flat {
                            {
                                let toggle_id = row.id.clone();
                                let open_row = row.clone();
                                let aside_row = row.clone();
                                let known_children = tree.get(&row.id);
                                let external = row.mode == crate::model::SubagentMode::External;
                                let expandable = !external
                                    && known_children.map(|list| !list.is_empty()).unwrap_or(true);
                                let is_open = expanded.iter().any(|id| id == &row.id);
                                rsx! {
                                    div {
                                        key: "{row.id}",
                                        class: "subagent-row",
                                        style: "padding-left: {depth * 14}px",
                                        button {
                                            class: "subagent-row__toggle",
                                            disabled: !expandable,
                                            title: if is_open { "收起" } else { "展开" },
                                            onclick: move |_| on_toggle.call(toggle_id.clone()),
                                            if expandable {
                                                Icon {
                                                    glyph: if is_open { "chevron-down-outline" } else { "chevron-right-outline" },
                                                    size: 12.0
                                                }
                                            }
                                        }
                                        button {
                                            class: "subagent-row__main",
                                            disabled: external,
                                            onclick: move |_| on_open.call(open_row.clone()),
                                            span { class: "{row.state.css_class()}" }
                                            span { class: "subagent-row__label", "{row.label}" }
                                            span { class: "{row.mode.css_class()}", "{row.mode.label()}" }
                                            span { class: "spacer" }
                                            span { class: "subagent-row__meta",
                                                "{format_duration(row.elapsed_ms)} · {format_tokens(row.tokens)}"
                                            }
                                        }
                                        button {
                                            class: "icon-btn subagent-row__aside",
                                            title: "{t.get(Key::OpenAside)}",
                                            disabled: external,
                                            onclick: move |_| on_open_aside.call(aside_row.clone()),
                                            Icon { glyph: "panel-right-outline".to_string(), size: 13.0 }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        SubagentGraph {
                            root_label: root_label.clone(),
                            rows: rows.clone(),
                            tree: tree.clone(),
                            on_toggle,
                            on_open,
                            on_open_aside,
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------- 关系图

const NODE_W: f64 = 196.0;
const NODE_H: f64 = 48.0;
const H_GAP: f64 = 22.0;
const V_GAP: f64 = 54.0;
const PAD: f64 = 16.0;

#[derive(Debug, Clone)]
struct GraphNode {
    id: String,
    row: Option<SubagentRow>,
    label: String,
    meta: String,
    mode: SubagentMode,
    state: SubagentState,
    is_root: bool,
    has_unloaded_children: bool,
    children: Vec<usize>,
    parent: Option<usize>,
    depth: usize,
    x: f64,
    y: f64,
    subtree_w: f64,
}

/// 只读的 tidy-tree 关系图。边来自父目录，节点来自已加载的 catalog。
#[component]
fn SubagentGraph(
    root_label: String,
    rows: Vec<SubagentRow>,
    tree: HashMap<String, Vec<SubagentRow>>,
    #[props(default = EventHandler::new(|_| {}))] on_toggle: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_open: EventHandler<SubagentRow>,
    #[props(default = EventHandler::new(|_| {}))] on_open_aside: EventHandler<SubagentRow>,
) -> Element {
    let t = use_t();
    let root_meta = format!("{} {}", rows.len(), t.get(Key::Subagents));
    let mut nodes = build_graph(&root_label, &root_meta, &rows, &tree);
    layout_graph(&mut nodes);

    let width = nodes
        .iter()
        .map(|node| node.x + NODE_W)
        .fold(NODE_W, f64::max)
        + PAD;
    let height = nodes
        .iter()
        .map(|node| node.y + NODE_H)
        .fold(NODE_H, f64::max)
        + PAD;
    let edges = graph_edges(&nodes);

    rsx! {
        div { class: "subagent-graph-scroll",
            div {
                class: "subagent-graph",
                style: "width: {width}px; height: {height}px;",
                svg {
                    class: "subagent-graph__edges",
                    width: "{width}",
                    height: "{height}",
                    view_box: "0 0 {width} {height}",
                    for edge in edges.iter() {
                        path { key: "{edge}", d: "{edge}", class: "sg-edge" }
                    }
                }
                for node in nodes.iter() {
                    if node.is_root {
                        div {
                            key: "{node.id}",
                            class: "subagent-node subagent-node--root",
                            style: "left: {node.x}px; top: {node.y}px;",
                            div { class: "subagent-node__main",
                                span { class: "dot dot--idle" }
                                div { class: "subagent-node__text",
                                    span { class: "subagent-node__label", "{node.label}" }
                                    span { class: "subagent-node__meta", "{node.meta}" }
                                }
                            }
                        }
                    } else {
                        {
                            let row = node.row.clone().expect("非根节点必须有 row");
                            let open_row = row.clone();
                            let aside_row = row.clone();
                            let toggle_id = row.id.clone();
                            let external = node.mode == SubagentMode::External;
                            let state_class = match node.state {
                                SubagentState::Running => "subagent-node--running",
                                SubagentState::Completed => "subagent-node--completed",
                                SubagentState::Idle => "subagent-node--idle",
                            };
                            let class = if external {
                                format!("subagent-node {state_class} subagent-node--external")
                            } else {
                                format!("subagent-node {state_class}")
                            };
                            rsx! {
                                div {
                                    key: "{node.id}",
                                    class: "{class}",
                                    style: "left: {node.x}px; top: {node.y}px;",
                                    button {
                                        class: "subagent-node__main",
                                        title: "{node.id}",
                                        disabled: external,
                                        onclick: move |_| on_open.call(open_row.clone()),
                                        span { class: "{node.state.css_class()}" }
                                        div { class: "subagent-node__text",
                                            span { class: "subagent-node__label", "{node.label}" }
                                            span { class: "subagent-node__meta",
                                                "{node.mode.label()} · {node.meta}"
                                            }
                                        }
                                    }
                                    if node.has_unloaded_children {
                                        button {
                                            class: "subagent-node__more",
                                            title: "{t.get(Key::LoadChildren)}",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                on_toggle.call(toggle_id.clone());
                                            },
                                            "+"
                                        }
                                    }
                                    if !external {
                                        button {
                                            class: "subagent-node__aside",
                                            title: "{t.get(Key::OpenAside)}",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                on_open_aside.call(aside_row.clone());
                                            },
                                            Icon { glyph: "panel-right-outline".to_string(), size: 11.0 }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn build_graph(
    root_label: &str,
    root_meta: &str,
    rows: &[SubagentRow],
    tree: &HashMap<String, Vec<SubagentRow>>,
) -> Vec<GraphNode> {
    let root_label = if root_label.trim().is_empty() {
        "Session".to_string()
    } else {
        root_label.to_string()
    };

    let mut nodes = vec![GraphNode {
        id: "__root__".to_string(),
        row: None,
        label: root_label,
        meta: root_meta.to_string(),
        mode: SubagentMode::Continuable,
        state: SubagentState::Idle,
        is_root: true,
        has_unloaded_children: false,
        children: Vec::new(),
        parent: None,
        depth: 0,
        x: 0.0,
        y: 0.0,
        subtree_w: NODE_W,
    }];

    let mut visited = std::collections::HashSet::new();
    let mut stack: Vec<(usize, SubagentRow)> =
        rows.iter().rev().map(|row| (0, row.clone())).collect();

    while let Some((parent, row)) = stack.pop() {
        if !visited.insert(row.id.clone()) {
            continue;
        }
        let idx = nodes.len();
        let external = row.mode == SubagentMode::External;
        let has_unloaded_children = !external && !tree.contains_key(&row.id);
        nodes.push(GraphNode {
            id: row.id.clone(),
            meta: format!(
                "{} · {}",
                format_duration(row.elapsed_ms),
                format_tokens(row.tokens)
            ),
            label: row.label.clone(),
            mode: row.mode,
            state: row.state,
            is_root: false,
            has_unloaded_children,
            children: Vec::new(),
            parent: Some(parent),
            depth: 0,
            x: 0.0,
            y: 0.0,
            subtree_w: NODE_W,
            row: Some(row.clone()),
        });
        nodes[parent].children.push(idx);

        if let Some(children) = tree.get(&row.id) {
            for child in children.iter().rev() {
                stack.push((idx, child.clone()));
            }
        }
    }

    nodes
}

fn layout_graph(nodes: &mut [GraphNode]) {
    if nodes.is_empty() {
        return;
    }
    measure(nodes, 0);
    place(nodes, 0, PAD, 0);
}

fn measure(nodes: &mut [GraphNode], index: usize) -> f64 {
    let children = nodes[index].children.clone();
    if children.is_empty() {
        nodes[index].subtree_w = NODE_W;
        return NODE_W;
    }

    let mut total = 0.0;
    for (offset, child) in children.iter().enumerate() {
        if offset > 0 {
            total += H_GAP;
        }
        total += measure(nodes, *child);
    }
    let width = total.max(NODE_W);
    nodes[index].subtree_w = width;
    width
}

fn place(nodes: &mut [GraphNode], index: usize, left: f64, depth: usize) {
    nodes[index].depth = depth;
    nodes[index].y = PAD + depth as f64 * (NODE_H + V_GAP);

    let children = nodes[index].children.clone();
    if children.is_empty() {
        nodes[index].x = left + (nodes[index].subtree_w - NODE_W) / 2.0;
        return;
    }

    let mut cursor = left;
    for child in children.iter() {
        place(nodes, *child, cursor, depth + 1);
        cursor += nodes[*child].subtree_w + H_GAP;
    }

    let first = nodes[children[0]].x + NODE_W / 2.0;
    let last = nodes[*children.last().expect("非空")].x + NODE_W / 2.0;
    nodes[index].x = (first + last) / 2.0 - NODE_W / 2.0;
}

fn graph_edges(nodes: &[GraphNode]) -> Vec<String> {
    nodes
        .iter()
        .filter_map(|node| {
            let parent = &nodes[node.parent?];
            let x1 = parent.x + NODE_W / 2.0;
            let y1 = parent.y + NODE_H;
            let x2 = node.x + NODE_W / 2.0;
            let y2 = node.y;
            let mid = (y1 + y2) / 2.0;
            Some(format!(
                "M {x1:.1} {y1:.1} C {x1:.1} {mid:.1}, {x2:.1} {mid:.1}, {x2:.1} {y2:.1}"
            ))
        })
        .collect()
}

/// 右栏侧开的子会话：只读消息流，不提供 composer。
#[component]
pub fn SubagentAside(chat: AsideChat, on_close: EventHandler<()>) -> Element {
    let t = use_t();
    let start = chat.messages.len().saturating_sub(200);

    rsx! {
        section { class: "aside-chat",
            header { class: "aside-chat__head",
                span { class: "aside-chat__title", "{chat.title}" }
                if chat.running {
                    span { class: "chip chip--live", "{t.get(Key::StatusRunning)}" }
                }
                span { class: "spacer" }
                button {
                    class: "icon-btn",
                    title: "{t.get(Key::Close)}",
                    onclick: move |_| on_close.call(()),
                    Icon { glyph: "close-outline".to_string(), size: 13.0 }
                }
            }
            div { class: "aside-chat__body",
                if chat.messages.is_empty() {
                    div { class: "muted", "{t.get(Key::NoMessages)}" }
                }
                for message in chat.messages.iter().skip(start) {
                    div { key: "{message.id}", class: "aside-msg",
                        span { class: "aside-msg__role muted", "{t.get(i18n::role_key(message.role))}" }
                        span { class: "aside-msg__text", "{message.text}" }
                    }
                }
            }
        }
    }
}

/// 把层级目录压平成 `(depth, row)`，只展开 `expanded` 里的节点。
fn flatten(
    tree: &HashMap<String, Vec<SubagentRow>>,
    rows: &[SubagentRow],
    expanded: &[String],
) -> Vec<(usize, SubagentRow)> {
    let mut out = Vec::new();
    walk(tree, rows, expanded, 0, &mut out);
    out
}

fn walk(
    tree: &HashMap<String, Vec<SubagentRow>>,
    rows: &[SubagentRow],
    expanded: &[String],
    depth: usize,
    out: &mut Vec<(usize, SubagentRow)>,
) {
    for row in rows {
        out.push((depth, row.clone()));
        if expanded.iter().any(|id| id == &row.id)
            && let Some(children) = tree.get(&row.id)
        {
            walk(tree, children, expanded, depth + 1, out);
        }
    }
}

/// 毫秒 → `1.2s` / `3m05s` / `1h02m`。
pub fn format_duration(ms: u64) -> String {
    if ms < 1_000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else if ms < 3_600_000 {
        let total = ms / 1000;
        format!("{}m{:02}s", total / 60, total % 60)
    } else {
        let total = ms / 60_000;
        format!("{}h{:02}m", total / 60, total % 60)
    }
}

/// token 数 → `980` / `1.2k` / `3.4M`。
pub fn format_tokens(tokens: u64) -> String {
    if tokens < 1_000 {
        tokens.to_string()
    } else if tokens < 1_000_000 {
        format!("{:.1}k", tokens as f64 / 1000.0)
    } else {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    }
}

/// 目录行的状态点（供调试与单测使用）。
pub fn state_label(state: SubagentState) -> &'static str {
    match state {
        SubagentState::Running => "running",
        SubagentState::Completed => "completed",
        SubagentState::Idle => "idle",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SubagentMode;

    fn row(id: &str, parent: &str) -> SubagentRow {
        SubagentRow {
            id: id.to_string(),
            parent_id: parent.to_string(),
            mode: SubagentMode::Continuable,
            label: id.to_string(),
            state: SubagentState::Idle,
            elapsed_ms: 0,
            tokens: 0,
        }
    }

    #[test]
    fn flatten_respects_expansion() {
        let mut tree = HashMap::new();
        tree.insert("a".to_string(), vec![row("a1", "a")]);
        let roots = vec![row("a", "root"), row("b", "root")];

        let collapsed = flatten(&tree, &roots, &[]);
        assert_eq!(collapsed.len(), 2);

        let expanded = flatten(&tree, &roots, &["a".to_string()]);
        assert_eq!(expanded.len(), 3);
        assert_eq!(expanded[1].0, 1);
        assert_eq!(expanded[1].1.id, "a1");
    }

    #[test]
    fn formats_duration_and_tokens() {
        assert_eq!(format_duration(320), "320ms");
        assert_eq!(format_duration(1_500), "1.5s");
        assert_eq!(format_duration(65_000), "1m05s");
        assert_eq!(format_tokens(980), "980");
        assert_eq!(format_tokens(1_200), "1.2k");
        assert_eq!(format_tokens(3_400_000), "3.4M");
    }

    #[test]
    fn graph_builds_tree_edges_and_positions() {
        let rows = vec![row("a", "root"), row("b", "root")];
        let mut tree = HashMap::new();
        tree.insert("a".to_string(), vec![row("a1", "a")]);

        let mut nodes = build_graph("root", "2 subagents", &rows, &tree);
        assert_eq!(nodes.len(), 4, "root + a + b + a1");

        let a = nodes.iter().position(|node| node.id == "a").unwrap();
        let b = nodes.iter().position(|node| node.id == "b").unwrap();
        let a1 = nodes.iter().position(|node| node.id == "a1").unwrap();
        assert_eq!(nodes[a1].parent, Some(a));
        assert_eq!(nodes[a1].depth, 0, "布局前 depth 尚未赋值");
        assert!(!nodes[a].has_unloaded_children);
        assert!(nodes[b].has_unloaded_children, "b 的目录尚未加载");

        layout_graph(&mut nodes);
        assert_eq!(nodes[0].depth, 0);
        assert_eq!(nodes[a].depth, 1);
        assert_eq!(nodes[a1].depth, 2);
        assert!(nodes[a].x < nodes[b].x, "兄弟节点按目录顺序从左到右");
        assert_eq!(graph_edges(&nodes).len(), 3, "除根外每个节点一条边");
        assert!(nodes[0].subtree_w >= 2.0 * NODE_W + H_GAP);
    }

    #[test]
    fn graph_layout_centers_root_and_aligns_levels() {
        let rows = vec![row("a", "root"), row("b", "root")];
        let mut tree = HashMap::new();
        tree.insert("a".to_string(), vec![row("a1", "a")]);

        let mut nodes = build_graph("root", "2 subagents", &rows, &tree);
        layout_graph(&mut nodes);

        let a = nodes.iter().position(|node| node.id == "a").unwrap();
        let b = nodes.iter().position(|node| node.id == "b").unwrap();
        let a1 = nodes.iter().position(|node| node.id == "a1").unwrap();
        let level_one_y = PAD + NODE_H + V_GAP;
        let level_two_y = level_one_y + NODE_H + V_GAP;

        assert!((nodes[a].y - level_one_y).abs() < 0.01);
        assert!((nodes[b].y - level_one_y).abs() < 0.01);
        assert!((nodes[a1].y - level_two_y).abs() < 0.01);
        let expected_root_x = (nodes[a].x + nodes[b].x) / 2.0;
        assert!((nodes[0].x - expected_root_x).abs() < 0.01, "根节点应居中");

        let edges = graph_edges(&nodes);
        assert_eq!(edges.len(), 3);
        let root_start = format!(
            "M {:.1} {:.1}",
            nodes[0].x + NODE_W / 2.0,
            nodes[0].y + NODE_H
        );
        let leaf_end = format!("{:.1} {:.1}", nodes[a1].x + NODE_W / 2.0, nodes[a1].y);
        assert!(edges.iter().any(|edge| edge.starts_with(&root_start)));
        assert!(edges.iter().any(|edge| edge.ends_with(&leaf_end)));
    }
}
