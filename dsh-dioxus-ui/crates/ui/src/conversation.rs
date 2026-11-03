//! 会话页：标题栏 + 消息流（对话 / 轨迹）+ 输入区。

use dioxus::prelude::*;

use crate::icons::Icon;
use crate::markdown::MarkdownView;
use crate::model::{ApprovalItem, ConnectionState, MessageRow, Role, SendMode, TrajectoryRow};
use crate::tool_card::ToolCard;

/// 单次渲染的消息上限。超出后只渲染尾部，保证长会话不卡。
const MAX_RENDER: usize = 300;

#[component]
pub fn ConversationView(
    active_id: Option<String>,
    messages: Vec<MessageRow>,
    connection: ConnectionState,
    #[props(default = Vec::new())] approvals: Vec<ApprovalItem>,
    #[props(default = Vec::new())] trajectory: Vec<TrajectoryRow>,
    #[props(default = false)] running: bool,
    #[props(default = SendMode::Queue)] send_mode: SendMode,
    #[props(default = EventHandler::new(|_| {}))] on_mode: EventHandler<SendMode>,
    #[props(default = EventHandler::new(|_| {}))] on_cancel: EventHandler<()>,
    #[props(default = EventHandler::new(|_| {}))] on_decide: EventHandler<(String, bool)>,
    on_send: EventHandler<String>,
    on_toggle_right: EventHandler<()>,
    #[props(default = String::from("对话"))] title: String,
) -> Element {
    let mut draft = use_signal(String::new);
    let mut tab = use_signal(|| 0u8);

    let message_count = messages.len();
    use_effect(move || {
        let _ = message_count;
        let _ = document::eval(
            "const el = document.querySelector('.messages'); if (el) el.scrollTop = el.scrollHeight;",
        );
    });

    if active_id.is_none() {
        return rsx! {
            section { class: "conversation conversation--empty",
                div { class: "empty",
                    h2 { "选择或新建一个会话" }
                    p { class: "muted", "左侧列表来自 Host 的 session/list。" }
                }
            }
        };
    }

    let can_send = connection == ConnectionState::Connected;
    let truncated = messages.len().saturating_sub(MAX_RENDER);

    rsx! {
        section { class: "conversation",
            header { class: "conversation__head",
                span { class: "conversation__title", "{title}" }
                div { class: "tabs",
                    button {
                        class: if tab() == 0 { "tab tab--active" } else { "tab" },
                        onclick: move |_| tab.set(0),
                        "对话"
                    }
                    button {
                        class: if tab() == 1 { "tab tab--active" } else { "tab" },
                        onclick: move |_| tab.set(1),
                        "轨迹 {trajectory.len()}"
                    }
                }
                span { class: "spacer" }
                button {
                    class: "icon-btn",
                    title: "右栏",
                    onclick: move |_| on_toggle_right.call(()),
                    Icon { glyph: "panel-right-outline".to_string() }
                }
            }

            if tab() == 0 {
                div { class: "messages",
                    if messages.is_empty() {
                        div { class: "empty muted", "还没有消息。发一条试试。" }
                    }
                    if truncated > 0 {
                        div { class: "muted truncation-hint",
                            "仅渲染最近 {MAX_RENDER} 条（共 {messages.len()} 条）"
                        }
                    }
                    for message in messages.iter().skip(truncated) {
                        MessageItem { key: "{message.id}", message: message.clone() }
                    }
                }
            } else {
                div { class: "messages trajectory",
                    if trajectory.is_empty() {
                        div { class: "empty muted", "暂无事件。" }
                    }
                    for row in trajectory.iter() {
                        div { key: "{row.seq}", class: "traj-row",
                            span { class: "traj-seq", "#{row.seq}" }
                            span { class: "traj-kind", "{row.kind}" }
                            span { class: "traj-detail muted", "{row.detail}" }
                        }
                    }
                }
            }

            if !approvals.is_empty() {
                div { class: "approval-bar",
                    for item in approvals.iter() {
                        {
                            let allow_id = item.event_id.clone();
                            let deny_id = item.event_id.clone();
                            rsx! {
                                div { key: "{item.event_id}", class: "approval-card",
                                    div { class: "approval-card__head",
                                        span { class: "chip chip--approval", "需要确认" }
                                        span { class: "approval-card__title", "{item.title}" }
                                        span { class: "muted", "{item.agent_id}" }
                                    }
                                    pre { class: "approval-card__detail", "{item.detail}" }
                                    div { class: "approval-card__actions",
                                        button {
                                            class: "primary-btn",
                                            onclick: move |_| on_decide.call((allow_id.clone(), true)),
                                            "允许一次"
                                        }
                                        button {
                                            class: "ghost-btn",
                                            onclick: move |_| on_decide.call((deny_id.clone(), false)),
                                            "拒绝"
                                        }
                                    }
                                }
                            }
                        }
                    }
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
                div { class: "composer__side",
                    div { class: "segmented",
                        button {
                            class: "{send_mode.css_class(send_mode == SendMode::Queue)}",
                            onclick: move |_| on_mode.call(SendMode::Queue),
                            "{SendMode::Queue.label()}"
                        }
                        button {
                            class: "{send_mode.css_class(send_mode == SendMode::Steer)}",
                            onclick: move |_| on_mode.call(SendMode::Steer),
                            "{SendMode::Steer.label()}"
                        }
                    }
                    if running {
                        button {
                            class: "ghost-btn ghost-btn--danger",
                            onclick: move |_| on_cancel.call(()),
                            "停止"
                        }
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
}

#[component]
fn MessageItem(message: MessageRow) -> Element {
    rsx! {
        article { class: match message.role {
            Role::User => "msg msg--user",
            Role::Assistant => "msg msg--assistant",
            Role::System => "msg msg--system",
            Role::Tool => "msg msg--tool",
        },
            div { class: "msg__head",
                span { class: "msg__role", "{message.role.label()}" }
                if message.streaming {
                    span { class: "chip chip--live", "流式中" }
                }
            }
            if message.role == Role::Tool {
                ToolCard {
                    name: message.tool_name.clone().unwrap_or_else(|| "tool".to_string()),
                    body: message.text.clone(),
                }
            } else {
                div { class: "msg__body",
                    MarkdownView { source: message.text.clone() }
                    if message.streaming {
                        span { class: "caret" }
                    }
                }
            }
        }
    }
}
