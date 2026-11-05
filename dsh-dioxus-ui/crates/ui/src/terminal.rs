//! 右栏用户终端面板。
//!
//! 只负责展示与收集输入：连接、创建、attach 都在 `dsh-app` 里完成。
//! 屏幕文本是 Host 下发的 ANSI 快照 + 增量，这里用 `dsh-render` 的
//! 最小解析器转成「行 → 样式片段」，不做光标建模。

use dioxus::prelude::*;

use crate::i18n::use_t;
use crate::icons::Icon;
use crate::model::TerminalView;

/// 单次最多挂载的行数（scrollback 上限之外的兜底）。
const MAX_LINES: usize = 2000;

#[component]
pub fn TerminalPanel(
    #[props(default = None)] terminal: Option<TerminalView>,
    #[props(default = false)] loading: bool,
    #[props(default = EventHandler::new(|_| {}))] on_open: EventHandler<()>,
    #[props(default = EventHandler::new(|_| {}))] on_close: EventHandler<()>,
    #[props(default = EventHandler::new(|_| {}))] on_input: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_copy: EventHandler<String>,
) -> Element {
    let t = use_t();

    let screen_len = terminal.as_ref().map(|view| view.screen.len()).unwrap_or(0);
    use_effect(move || {
        let _ = screen_len;
        let _ = document::eval(
            "const el = document.querySelector('.terminal__screen'); if (el) el.scrollTop = el.scrollHeight;",
        );
    });

    let Some(view) = terminal else {
        return rsx! {
            div { class: "terminal terminal--empty",
                div { class: "empty",
                    h3 { "{t.tr(\"终端\", \"Terminal\")}" }
                    p { class: "muted", "{t.tr(\"在当前会话的工作目录里打开一个交互式 shell。\", \"Open an interactive shell in this session's working directory.\")}" }
                    button {
                        class: "primary-btn",
                        disabled: loading,
                        onclick: move |_| on_open.call(()),
                        if loading {
                            "{t.tr(\"正在打开…\", \"Opening…\")}"
                        } else {
                            "{t.tr(\"打开终端\", \"Open terminal\")}"
                        }
                    }
                }
            }
        };
    };

    let running = view.is_running();
    let status_label = match view.state.as_str() {
        "running" => t.tr("运行中", "Running"),
        "exited" => t.tr("已退出", "Exited"),
        "failed" => t.tr("已失败", "Failed"),
        other => other,
    };
    let meta = if view.cols > 0 && view.rows > 0 {
        format!("{}×{}", view.cols, view.rows)
    } else {
        String::new()
    };

    let mut lines = dsh_render::parse_ansi(&view.screen);
    if lines.len() > MAX_LINES {
        lines.drain(..lines.len() - MAX_LINES);
    }
    let copy_text = lines
        .iter()
        .map(dsh_render::AnsiLine::text)
        .collect::<Vec<_>>()
        .join("\n");
    let input_disabled = !running || !view.writable;

    rsx! {
        div { class: "terminal",
            div { class: "terminal__head",
                span { class: if running { "dot dot--running" } else { "dot dot--idle" } }
                span { class: "terminal__title", "{view.title}" }
                span { class: "terminal__status muted", "{status_label}" }
                if !meta.is_empty() {
                    span { class: "terminal__status muted mono", "{meta}" }
                }
                span { class: "spacer" }
                if running {
                    button {
                        class: "ghost-btn",
                        title: t.tr("发送 Ctrl+C", "Send Ctrl+C"),
                        onclick: move |_| on_input.call("\u{3}".to_string()),
                        "^C"
                    }
                }
                button {
                    class: "ghost-btn",
                    title: t.tr("复制屏幕文本", "Copy screen text"),
                    onclick: move |_| on_copy.call(copy_text.clone()),
                    "{t.tr(\"复制\", \"Copy\")}"
                }
                if !running {
                    button {
                        class: "ghost-btn",
                        disabled: loading,
                        onclick: move |_| on_open.call(()),
                        "{t.tr(\"重新打开\", \"Reopen\")}"
                    }
                }
                button {
                    class: "icon-btn",
                    title: t.tr("关闭终端", "Close terminal"),
                    onclick: move |_| on_close.call(()),
                    Icon { glyph: "close-outline".to_string(), size: 12.0 }
                }
            }
            if let Some(error) = view.error.clone() {
                div { class: "terminal__error", "{error}" }
            }
            pre {
                class: "terminal__screen",
                // 终端习惯：点屏幕任意位置即可继续输入
                onclick: move |_| {
                    let _ = document::eval(
                        "document.querySelector('.terminal__field')?.focus()",
                    );
                },
                for (index, line) in lines.iter().enumerate() {
                    div { key: "{index}", class: "terminal__line",
                        for (span_index, span) in line.spans.iter().enumerate() {
                            span {
                                key: "{span_index}",
                                style: "{span.style.css()}",
                                "{span.text}"
                            }
                        }
                    }
                }
            }
            TerminalInputBar { writable: !input_disabled, on_input }
        }
    }
}

/// 命令行输入：独立组件，避免每次按键都重渲染整屏终端。
#[component]
fn TerminalInputBar(writable: bool, on_input: EventHandler<String>) -> Element {
    let t = use_t();
    let mut draft = use_signal(String::new);

    // 终端 tab 可见时自动聚焦输入行（隐藏时 offsetParent 为空，不会抢聊天输入框焦点）
    use_effect(move || {
        let _ = document::eval(
            "const el = document.querySelector('.terminal__field'); if (el && el.offsetParent !== null && document.activeElement !== el) el.focus();",
        );
    });

    rsx! {
        div { class: "terminal__input",
            span { class: "terminal__prompt mono", "$" }
            input {
                class: "terminal__field mono",
                r#type: "text",
                value: "{draft}",
                disabled: !writable,
                placeholder: if writable {
                    t.tr("输入命令，回车发送", "Type a command, press Enter").to_string()
                } else {
                    t.tr("进程已结束", "Process exited").to_string()
                },
                oninput: move |evt| draft.set(evt.value()),
                onkeydown: move |evt| {
                    if evt.key().to_string() != "Enter" || !writable {
                        return;
                    }
                    let value = draft.read().trim_end().to_string();
                    if value.is_empty() {
                        return;
                    }
                    on_input.call(format!("{value}\n"));
                    draft.set(String::new());
                },
            }
            button {
                class: "icon-btn",
                disabled: !writable || draft.read().trim().is_empty(),
                title: t.tr("发送", "Send"),
                onclick: move |_| {
                    if !writable {
                        return;
                    }
                    let value = draft.read().trim_end().to_string();
                    if value.is_empty() {
                        return;
                    }
                    on_input.call(format!("{value}\n"));
                    draft.set(String::new());
                },
                Icon { glyph: "send-outline".to_string() }
            }
        }
    }
}
