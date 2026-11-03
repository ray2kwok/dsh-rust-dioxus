//! 会话页：标题栏 + 消息流 + 输入区。
//!
//! 对应上游 `ui-conversation` 的外壳与 `ui-chat` 的消息列表。
//! 本阶段只做纯文本渲染；Markdown / 工具卡 / diff 在 M1 接入 `dsh-render`。

use dioxus::prelude::*;

use crate::model::{ConnectionState, MessageRow, Role};

#[component]
pub fn ConversationView(
    active_id: Option<String>,
    messages: Vec<MessageRow>,
    connection: ConnectionState,
    on_send: EventHandler<String>,
    on_toggle_right: EventHandler<()>,
) -> Element {
    let mut draft = use_signal(String::new);

    // 新消息到达后滚到底部（命令式副作用只允许出现在 effect 里）
    let message_count = messages.len();
    use_effect(move || {
        let _ = message_count;
        let _ = document::eval(
            "const el = document.querySelector('.messages'); if (el) el.scrollTop = el.scrollHeight;",
        );
    });

    let Some(session_id) = active_id.clone() else {
        return rsx! {
            section { class: "conversation conversation--empty",
                div { class: "empty",
                    h2 { "选择或新建一个会话" }
                    p { class: "muted", "左侧列表来自 Host 的 session/list。" }
                }
            }
        };
    };

    let can_send = connection == ConnectionState::Connected;

    rsx! {
        section { class: "conversation",
            header { class: "conversation__head",
                span { class: "conversation__title", "{session_id}" }
                span { class: "spacer" }
                button {
                    class: "icon-btn",
                    title: "右栏",
                    onclick: move |_| on_toggle_right.call(()),
                    "▥"
                }
            }

            div { class: "messages",
                if messages.is_empty() {
                    div { class: "empty muted", "还没有消息。发一条试试。" }
                }
                for message in messages {
                    MessageItem { key: "{message.id}", message }
                }
            }

            footer { class: "composer",
                textarea {
                    class: "composer__input",
                    rows: "3",
                    placeholder: if can_send { "输入消息，Enter 发送 / Shift+Enter 换行" } else { "未连接，暂不可发送" },
                    disabled: !can_send,
                    value: "{draft}",
                    oninput: move |evt| draft.set(evt.value()),
                    onkeydown: move |evt| {
                        if evt.key().to_string() == "Enter" && !evt.modifiers().shift() {
                            evt.prevent_default();
                            let text = draft();
                            if !text.trim().is_empty() {
                                on_send.call(text);
                                draft.set(String::new());
                            }
                        }
                    },
                }
                button {
                    class: "primary-btn",
                    disabled: !can_send || draft().trim().is_empty(),
                    onclick: move |_| {
                        let text = draft();
                        if !text.trim().is_empty() {
                            on_send.call(text);
                            draft.set(String::new());
                        }
                    },
                    "发送"
                }
            }
        }
    }
}

#[component]
fn MessageItem(message: MessageRow) -> Element {
    let class = match message.role {
        Role::User => "msg msg--user",
        Role::Assistant => "msg msg--assistant",
        Role::System => "msg msg--system",
        Role::Tool => "msg msg--tool",
    };

    rsx! {
        article { class: "{class}",
            div { class: "msg__head",
                span { class: "msg__role", "{message.role.label()}" }
                if let Some(tool) = message.tool_name.clone() {
                    span { class: "chip", "{tool}" }
                }
                if message.streaming {
                    span { class: "chip chip--live", "流式中" }
                }
            }
            div { class: "msg__body",
                "{message.text}"
                if message.streaming {
                    span { class: "caret" }
                }
            }
        }
    }
}
