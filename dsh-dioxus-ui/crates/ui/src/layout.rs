//! 三栏骨架：Sidebar / Main / RightBar。
//!
//! 对应上游 `ui-layout` 的 `AppFrame`（264–420px 侧栏、可折叠、右栏可关）。
//! 本阶段固定宽度 + 折叠开关，拖拽调宽留到 M3。

use dioxus::prelude::*;

use crate::conversation::ConversationView;
use crate::model::{ConnectionState, MessageRow, SessionSummary};
use crate::sidebar::SessionList;

/// 三栏外壳。数据全部由外部传入，组件自身不持有业务状态。
#[component]
pub fn AppShell(
    sessions: Vec<SessionSummary>,
    active_id: Option<String>,
    messages: Vec<MessageRow>,
    connection: ConnectionState,
    /// 连接代次（`$events` ready 后的计数），M0 阶段显示在状态栏。
    #[props(default = 0)]
    generation: u64,
    /// 状态栏里的 Host 地址。
    #[props(default = String::new())]
    host_label: String,
    on_select: EventHandler<String>,
    on_send: EventHandler<String>,
) -> Element {
    let mut collapsed = use_signal(|| false);
    let mut show_right = use_signal(|| false);

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
                        if collapsed() { "»" } else { "«" }
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
                    messages,
                    connection,
                    on_send,
                    on_toggle_right: move |_| show_right.toggle(),
                }
            }

            if show_right() {
                aside { class: "rightbar",
                    div { class: "rightbar__head",
                        span { "文件" }
                        button {
                            class: "icon-btn",
                            onclick: move |_| show_right.set(false),
                            "×"
                        }
                    }
                    div { class: "rightbar__body muted", "文件树 / 文档预览（M4）" }
                }
            }
        }

        footer { class: "statusbar",
            span { class: "{connection.css_class()}", "{connection.label()}" }
            span { class: "muted", "{host_label}" }
            span { class: "spacer" }
            span { class: "muted", "generation #{generation}" }
            span { class: "muted", "dsh-dioxus-ui" }
        }
    }
}
