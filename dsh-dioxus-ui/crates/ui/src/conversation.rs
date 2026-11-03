//! 会话页：标题栏 + 消息流（对话 / 轨迹）+ 输入区。

use std::collections::HashMap;

use dioxus::prelude::*;

use crate::i18n::{self, Key, use_t};
use crate::icons::Icon;
use crate::markdown::MarkdownView;
use crate::model::{
    ApprovalItem, ConnectionState, MessageRow, QuestionCard, QueueItem, Role, SendMode,
    SubagentMode, SubagentRow, TrajectoryRow,
};
use crate::subagent::SubagentCatalog;
use crate::tool_card::ToolCard;

/// 单次渲染上限：超过后只渲染尾部，避免一次挂载几千个节点。
///
/// 更早的历史由 `session/page` 按需拉取（`on_load_earlier`），
/// 没有做固定高度虚拟列表——消息高度差异太大，估高会导致滚动跳动。
const MAX_RENDER: usize = 400;

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
    #[props(default = Vec::new())] questions: Vec<QuestionCard>,
    #[props(default = Vec::new())] queue: Vec<QueueItem>,
    /// 上下文占用（surfaceTokens, contextWindow）
    #[props(default = None)]
    context_usage: Option<(u64, u64)>,
    #[props(default = EventHandler::new(|_| {}))] on_queue_action: EventHandler<(String, bool)>,
    /// 还有更早的历史可拉
    #[props(default = false)]
    has_more: bool,
    #[props(default = EventHandler::new(|_| {}))] on_load_earlier: EventHandler<()>,
    /// (event_id, selected labels, custom text)
    #[props(default = EventHandler::new(|_| {}))]
    on_answer: EventHandler<(String, Vec<String>, String)>,
    #[props(default = EventHandler::new(|_| {}))] on_rename: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_archive: EventHandler<()>,
    on_send: EventHandler<String>,
    on_toggle_right: EventHandler<()>,
    #[props(default = String::from("对话"))] title: String,
    /// 当前会话是否是子会话。
    #[props(default = false)]
    is_subagent: bool,
    /// 子会话的可继续性；普通会话为 `None`。
    #[props(default = None)]
    subagent_mode: Option<SubagentMode>,
    /// 父会话是否可用（决定 continuable 子会话能否续聊）。
    #[props(default = true)]
    parent_available: bool,
    /// 祖先面包屑 `(session_id, title)`。
    #[props(default = Vec::new())]
    lineage: Vec<(String, String)>,
    #[props(default = EventHandler::new(|_| {}))] on_navigate: EventHandler<String>,
    /// 当前父会话的直接子 Agent 目录。
    #[props(default = Vec::new())]
    subagent_rows: Vec<SubagentRow>,
    /// 已按需加载的嵌套目录（parent id → children）。
    #[props(default = HashMap::new())]
    subagent_tree: HashMap<String, Vec<SubagentRow>>,
    #[props(default = Vec::new())] subagent_expanded: Vec<String>,
    #[props(default = EventHandler::new(|_| {}))] on_subagent_toggle: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_subagent_open: EventHandler<SubagentRow>,
    #[props(default = EventHandler::new(|_| {}))] on_subagent_aside: EventHandler<SubagentRow>,
    #[props(default = EventHandler::new(|_| {}))] on_subagent_refresh: EventHandler<()>,
) -> Element {
    let t = use_t();
    let mut draft = use_signal(String::new);
    let mut tab = use_signal(|| 0u8);
    let mut renaming = use_signal(|| false);
    let mut title_draft = use_signal(String::new);

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
                    h2 { "{t.get(Key::SelectSession)}" }
                    p { class: "muted", "{t.get(Key::SelectSessionHint)}" }
                }
            }
        };
    }

    let can_send = connection == ConnectionState::Connected;
    let truncated = messages.len().saturating_sub(MAX_RENDER);
    // 子会话的只读判定：one-shot / unknown / external 一律只读；
    // continuable 在父会话不可用时也只读，但仍在运行时保留独立停止。
    let catalog_root_label = title.clone();

    let read_only_reason = match subagent_mode {
        Some(SubagentMode::Continuable) if !parent_available => {
            Some(Key::SubagentParentUnavailable)
        }
        Some(SubagentMode::OneShot) => Some(Key::SubagentReadOnlyOneShot),
        Some(SubagentMode::External) => Some(Key::SubagentReadOnlyExternal),
        Some(SubagentMode::Unknown) => Some(Key::SubagentReadOnlyUnknown),
        _ => None,
    };

    rsx! {
        section { class: "conversation",
            header { class: "conversation__head",
                if !lineage.is_empty() {
                    nav { class: "breadcrumb",
                        for (id, label) in lineage.iter() {
                            {
                                let target = id.clone();
                                rsx! {
                                    button {
                                        key: "{id}",
                                        class: "breadcrumb__item",
                                        title: "{label}",
                                        onclick: move |_| on_navigate.call(target.clone()),
                                        "{label}"
                                    }
                                    span { class: "breadcrumb__sep", "/" }
                                }
                            }
                        }
                    }
                }
                if is_subagent {
                    span { class: "conversation__title", "{title}" }
                } else if renaming() {
                    input {
                        class: "title-input",
                        value: "{title_draft}",
                        oninput: move |evt| title_draft.set(evt.value()),
                        onkeydown: move |evt| match evt.key().to_string().as_str() {
                            "Enter" => {
                                let value = title_draft();
                                if !value.trim().is_empty() {
                                    on_rename.call(value);
                                }
                                renaming.set(false);
                            }
                            "Escape" => renaming.set(false),
                            _ => {}
                        },
                    }
                } else {
                    button {
                        class: "conversation__title",
                        title: "{t.get(Key::RenameHint)}",
                        onclick: move |_| {
                            title_draft.set(title.clone());
                            renaming.set(true);
                        },
                        "{title}"
                    }
                }
                div { class: "tabs",
                    button {
                        class: if tab() == 0 { "tab tab--active" } else { "tab" },
                        onclick: move |_| tab.set(0),
                        "{t.get(Key::Chat)}"
                    }
                    button {
                        class: if tab() == 1 { "tab tab--active" } else { "tab" },
                        onclick: move |_| tab.set(1),
                        "{t.get(Key::Trajectory)} {trajectory.len()}"
                    }
                }
                SubagentCatalog {
                    rows: subagent_rows.clone(),
                    tree: subagent_tree.clone(),
                    expanded: subagent_expanded.clone(),
                    root_label: catalog_root_label.clone(),
                    on_toggle: on_subagent_toggle,
                    on_open: on_subagent_open,
                    on_open_aside: on_subagent_aside,
                    on_refresh: on_subagent_refresh,
                }
                span { class: "spacer" }
                if let Some((used, window)) = context_usage {
                    {
                        let percent = ((used as f64 / window as f64) * 100.0).round() as u64;
                        rsx! {
                            span {
                                class: if percent >= 85 { "meter meter--hot" } else { "meter" },
                                title: "{used} / {window} tokens",
                                "{percent}%"
                            }
                        }
                    }
                }
                if !is_subagent {
                    button {
                        class: "icon-btn",
                        title: "{t.get(Key::Archive)}",
                        onclick: move |_| on_archive.call(()),
                        Icon { glyph: "branch-outline".to_string() }
                    }
                }
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
                        div { class: "empty muted", "{t.get(Key::NoMessages)}" }
                    }
                    if has_more {
                        button {
                            class: "load-earlier",
                            onclick: move |_| on_load_earlier.call(()),
                            "{t.get(Key::LoadEarlier)}"
                        }
                    }
                    for message in messages.iter().skip(truncated) {
                        MessageItem { key: "{message.id}", message: message.clone() }
                    }
                }
            } else {
                div { class: "messages trajectory",
                    if trajectory.is_empty() {
                        div { class: "empty muted", "{t.get(Key::Trajectory)} · 0" }
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
                                        span { class: "chip chip--approval", "{t.get(Key::NeedsConfirm)}" }
                                        span { class: "approval-card__title", "{item.title}" }
                                        span { class: "muted", "{item.agent_id}" }
                                    }
                                    pre { class: "approval-card__detail", "{item.detail}" }
                                    div { class: "approval-card__actions",
                                        button {
                                            class: "primary-btn",
                                            onclick: move |_| on_decide.call((allow_id.clone(), true)),
                                            "{t.get(Key::AllowOnce)}"
                                        }
                                        button {
                                            class: "ghost-btn",
                                            onclick: move |_| on_decide.call((deny_id.clone(), false)),
                                            "{t.get(Key::Reject)}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if !queue.is_empty() {
                div { class: "queue-strip",
                    span { class: "queue-strip__label muted", "{queue.len()}" }
                    for item in queue.iter() {
                        {
                            let remove_id = item.id.clone();
                            let steer_id = item.id.clone();
                            rsx! {
                                div { key: "{item.id}", class: "queue-item",
                                    span { class: "queue-item__text", "{item.text}" }
                                    button {
                                        class: "icon-btn",
                                        title: "{t.get(Key::Steer)}",
                                        onclick: move |_| on_queue_action.call((steer_id.clone(), true)),
                                        Icon { glyph: "send-outline".to_string(), size: 14.0 }
                                    }
                                    button {
                                        class: "icon-btn",
                                        title: "{t.get(Key::Reject)}",
                                        onclick: move |_| on_queue_action.call((remove_id.clone(), false)),
                                        Icon { glyph: "close-outline".to_string(), size: 14.0 }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            for card in questions.iter() {
                QuestionCardView { key: "{card.event_id}", card: card.clone(), on_answer }
            }

            footer { class: if read_only_reason.is_some() { "composer composer--readonly" } else { "composer" },
                if let Some(reason) = read_only_reason {
                    div { class: "composer__notice", "{t.get(reason)}" }
                    if subagent_mode.map(SubagentMode::is_continuable).unwrap_or(false) && running {
                        button {
                            class: "ghost-btn ghost-btn--danger",
                            onclick: move |_| on_cancel.call(()),
                            "{t.get(Key::Stop)}"
                        }
                    }
                } else {
                    textarea {
                        class: "composer__input",
                        rows: "3",
                        placeholder: if can_send { t.get(Key::ComposerPlaceholder) } else { t.get(Key::ComposerDisabled) },
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
                                "{t.get(Key::Queue)}"
                            }
                            button {
                                class: "{send_mode.css_class(send_mode == SendMode::Steer)}",
                                onclick: move |_| on_mode.call(SendMode::Steer),
                                "{t.get(Key::Steer)}"
                            }
                        }
                        if running {
                            button {
                                class: "ghost-btn ghost-btn--danger",
                                onclick: move |_| on_cancel.call(()),
                                "{t.get(Key::Stop)}"
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
                            "{t.get(Key::Send)}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MessageItem(message: MessageRow) -> Element {
    let t = use_t();
    let role_label = t.get(i18n::role_key(message.role));

    rsx! {
        article { class: match message.role {
            Role::User => "msg msg--user",
            Role::Assistant => "msg msg--assistant",
            Role::System => "msg msg--system",
            Role::Tool => "msg msg--tool",
        },
            div { class: "msg__head",
                span { class: "msg__role", "{role_label}" }
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

/// 提问卡：选项 + 可选的自定义回答。
#[component]
fn QuestionCardView(
    card: QuestionCard,
    on_answer: EventHandler<(String, Vec<String>, String)>,
) -> Element {
    let t = use_t();
    let mut selected = use_signal(Vec::<String>::new);
    let mut custom = use_signal(String::new);
    let first = card.items.first().cloned();
    let multi = first
        .as_ref()
        .map(|item| item.multi_select)
        .unwrap_or(false);

    rsx! {
        div { class: "approval-bar",
            div { class: "question-card",
                div { class: "approval-card__head",
                    span { class: "chip chip--question", "{t.get(Key::NeedsAnswer)}" }
                    span { class: "approval-card__title",
                        {first.as_ref().map(|item| item.question.clone()).unwrap_or_else(|| "请确认".to_string())}
                    }
                }
                if let Some(detail) = first.as_ref().and_then(|item| item.detail.clone()) {
                    pre { class: "approval-card__detail", "{detail}" }
                }
                if let Some(item) = first.clone() {
                    div { class: "question-options",
                        for option in item.options.iter() {
                            {
                                let label = option.clone();
                                let active = selected.read().contains(&label);
                                let pick = label.clone();
                                rsx! {
                                    button {
                                        key: "{label}",
                                        class: if active { "option option--active" } else { "option" },
                                        onclick: move |_| {
                                            let mut list = selected.write();
                                            if multi {
                                                if let Some(pos) = list.iter().position(|v| v == &pick) {
                                                    list.remove(pos);
                                                } else {
                                                    list.push(pick.clone());
                                                }
                                            } else {
                                                list.clear();
                                                list.push(pick.clone());
                                            }
                                        },
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                    input {
                        class: "question-custom",
                        placeholder: "{t.get(Key::OtherPlaceholder)}",
                        value: "{custom}",
                        oninput: move |evt| custom.set(evt.value()),
                    }
                }
                div { class: "approval-card__actions",
                    button {
                        class: "primary-btn",
                        onclick: {
                            let event_id = card.event_id.clone();
                            move |_| on_answer.call((
                                event_id.clone(),
                                selected.read().clone(),
                                custom.read().clone(),
                            ))
                        },
                        "{t.get(Key::SubmitAnswer)}"
                    }
                }
            }
        }
    }
}
