//! 左栏：按工作区分组的会话列表。
//!
//! 对应上游 `ui-sidebar` 的 `SidebarRoot`：workspace 分组 + 会话项 + 状态点。
//! 虚拟列表留到 M5（届时换成自研 `use_virtual_list`）。

use dioxus::prelude::*;

use crate::i18n::{Key, use_t};
use crate::icons::Icon;
use crate::model::SessionSummary;

#[component]
pub fn SessionList(
    sessions: Vec<SessionSummary>,
    active_id: Option<String>,
    on_select: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_new: EventHandler<()>,
) -> Element {
    let t = use_t();

    // 按 workspace 分组，保持首次出现顺序
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

    rsx! {
        nav { class: "session-list",
            button {
                class: "primary-btn primary-btn--block",
                onclick: move |_| on_new.call(()),
                Icon { glyph: "new-chat-outline".to_string() }
                span { "{t.get(Key::NewSession)}" }
            }
            for (workspace, items) in groups {
                div { class: "group",
                    div { class: "group__title", "{workspace}" }
                    for session in items {
                        {
                            let id = session.id.clone();
                            let selected = active_id.as_deref() == Some(session.id.as_str());
                            let class = if selected { "session-item session-item--active" } else { "session-item" };
                            rsx! {
                                button {
                                    key: "{session.id}",
                                    class: "{class}",
                                    onclick: move |_| on_select.call(id.clone()),
                                    span { class: "{session.status.css_class()}" }
                                    span { class: "session-item__title", "{session.title}" }
                                    span { class: "session-item__time muted", "{session.updated}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
