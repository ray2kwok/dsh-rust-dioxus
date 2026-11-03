//! 工具调用卡片。
//!
//! 对应上游 `ui-tool` 的 `tool.call.toolview` 分发：按 wire tool name 选专属卡片，
//! 未注册的工具落到通用卡片。当前只有通用卡片；专属卡片（bash/read/diff…）
//! 等真实工具事件接入后逐个补，接口保持不变。

use dioxus::prelude::*;

/// 通用工具卡：名称 + 可折叠正文。
#[component]
pub fn ToolCard(name: String, body: String) -> Element {
    // 长输出默认折叠，短输出默认展开
    let long = body.lines().count() > 12 || body.chars().count() > 600;
    let mut expanded = use_signal(move || !long);

    let summary = summarize(&body);

    rsx! {
        div { class: "tool-card",
            button {
                class: "tool-card__head",
                onclick: move |_| expanded.toggle(),
                span { class: "chip chip--tool", "{name}" }
                span { class: "tool-card__summary muted", "{summary}" }
                span { class: "tool-card__toggle muted", if expanded() { "收起" } else { "展开" } }
            }
            if expanded() {
                pre { class: "tool-card__body", "{body}" }
            }
        }
    }
}

/// 折叠时显示的第一行摘要。
fn summarize(body: &str) -> String {
    let first = body.lines().next().unwrap_or_default().trim();
    let mut text: String = first.chars().take(80).collect();
    if first.chars().count() > 80 {
        text.push('…');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::summarize;

    #[test]
    fn summary_takes_first_line_and_truncates() {
        assert_eq!(summarize("line1\nline2"), "line1");
        let long = "x".repeat(100);
        assert!(summarize(&long).chars().count() <= 81);
    }
}
