//! 中间编辑区：对话、文件预览与子会话只读副本，以 tab 形式共存。
//!
//! 对应 CodeBuddy 布局里的「编辑器组」：会话本身是第一个 tab，
//! 从左侧资源管理器打开文件后追加预览 tab，子会话入口追加只读 tab。

use dioxus::prelude::*;

use crate::i18n::use_t;
use crate::icons::Icon;
use crate::model::{AsideChat, PreviewDoc};
use crate::subagent::SubagentAside;

#[component]
pub fn EditorPane(
    /// 0 = 对话，1 = 文件预览，2 = 子会话。
    #[props(default = 0u8)]
    tab: u8,
    on_tab: EventHandler<u8>,
    /// 对话 tab 的内容（由布局层传入，避免把 50 多个会话 props 再复制一遍）。
    children: Element,
    #[props(default = None)] preview: Option<PreviewDoc>,
    #[props(default = None)] aside: Option<AsideChat>,
    #[props(default = String::new())] session_title: String,
    #[props(default = EventHandler::new(|_| {}))] on_copy: EventHandler<String>,
    #[props(default = EventHandler::new(|_| {}))] on_close_preview: EventHandler<()>,
    #[props(default = EventHandler::new(|_| {}))] on_close_aside: EventHandler<()>,
) -> Element {
    let t = use_t();
    let conversation_label = if session_title.is_empty() {
        t.tr("对话", "Chat").to_string()
    } else {
        session_title.clone()
    };

    let preview_name = preview
        .as_ref()
        .map(|doc| doc.path.rsplit('/').next().unwrap_or(&doc.path).to_string());
    let aside_name = aside.as_ref().map(|chat| chat.title.clone());
    let active = if (tab == 1 && preview.is_none()) || (tab == 2 && aside.is_none()) {
        0
    } else {
        tab
    };

    rsx! {
        section { class: "editor",
            div { class: "editor__tabs",
                div {
                    class: if active == 0 { "editor__tab editor__tab--active" } else { "editor__tab" },
                    onclick: move |_| {
                        on_tab.call(0);
                        // 从预览切回对话时，新挂载的列表需要贴底
                        let _ = document::eval(
                            "setTimeout(() => { const el = document.querySelector('.editor__conversation .messages'); if (el) el.scrollTop = el.scrollHeight; }, 40)",
                        );
                    },
                    Icon { glyph: "new-chat-outline".to_string(), size: 12.0 }
                    span { class: "editor__tab-label", "{conversation_label}" }
                }
                if let Some(name) = preview_name.clone() {
                    div {
                        class: if active == 1 { "editor__tab editor__tab--active" } else { "editor__tab" },
                        onclick: move |_| on_tab.call(1),
                        Icon { glyph: "folder-outline".to_string(), size: 12.0 }
                        span { class: "editor__tab-label", "{name}" }
                        button {
                            class: "editor__tab-close",
                            title: t.tr("关闭预览", "Close preview"),
                            onclick: move |evt| {
                                evt.stop_propagation();
                                on_close_preview.call(());
                            },
                            Icon { glyph: "close-outline".to_string(), size: 10.0 }
                        }
                    }
                }
                if let Some(name) = aside_name.clone() {
                    div {
                        class: if active == 2 { "editor__tab editor__tab--active" } else { "editor__tab" },
                        onclick: move |_| on_tab.call(2),
                        Icon { glyph: "branch-outline".to_string(), size: 12.0 }
                        span { class: "editor__tab-label", "{name}" }
                        button {
                            class: "editor__tab-close",
                            title: t.tr("关闭", "Close"),
                            onclick: move |evt| {
                                evt.stop_propagation();
                                on_close_aside.call(());
                            },
                            Icon { glyph: "close-outline".to_string(), size: 10.0 }
                        }
                    }
                }
            }

            if active == 0 {
                div { class: "editor__conversation", {children} }
            } else if active == 1 {
                if let Some(doc) = preview.clone() {
                    {
                        let lines = if doc.error {
                            Vec::new()
                        } else {
                            dsh_render::highlight(&doc.text, lang_of(&doc.path))
                        };
                        let copy_text = doc.text.clone();
                        rsx! {
                            div { class: "editor__breadcrumb",
                                span { class: "mono editor__path", "{doc.path}" }
                                span { class: "spacer" }
                                button {
                                    class: "editor__action",
                                    onclick: move |_| on_copy.call(copy_text.clone()),
                                    "{t.tr(\"复制\", \"Copy\")}"
                                }
                            }
                            if doc.error {
                                pre { class: "editor__plain", "{doc.text}" }
                            } else {
                                pre { class: "editor__code",
                                    code {
                                        for (index, line) in lines.iter().enumerate() {
                                            div { key: "{index}", class: "code-line",
                                                span { class: "code-no", "{index + 1}" }
                                                for (span_index, span) in line.iter().enumerate() {
                                                    span {
                                                        key: "{span_index}",
                                                        class: "{span.token.css_class()}",
                                                        "{span.text}"
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
            } else if active == 2 {
                if let Some(chat) = aside.clone() {
                    div { class: "editor__aside",
                        SubagentAside { chat, on_close: on_close_aside }
                    }
                }
            } else {
                div { class: "editor editor--empty",
                    div { class: "editor__welcome",
                        Icon { glyph: "folder-outline".to_string(), size: 40.0 }
                        h2 { "dsh" }
                        p { class: "muted", "{t.tr(\"从左侧资源管理器打开文件，或点击对话里的文件引用。\", \"Open a file from the explorer, or click a file reference in the conversation.\")}" }
                        div { class: "editor__hints muted",
                            span { "{t.tr(\"只读\", \"Read-only\")}" }
                            span { "{t.tr(\"跟随会话工作区\", \"Scoped to the session workspace\")}" }
                        }
                    }
                }
            }
        }
    }
}

/// 按扩展名挑语法高亮语言；未知类型返回 `None`（按纯文本渲染）。
fn lang_of(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "json" => "json",
        "py" => "python",
        "go" => "go",
        "c" | "h" => "c",
        "cc" | "cpp" | "hpp" => "cpp",
        "java" => "java",
        "sh" | "bash" | "zsh" => "bash",
        "toml" => "toml",
        "yml" | "yaml" => "yaml",
        "css" => "css",
        "html" => "html",
        "md" | "markdown" => "markdown",
        _ => return None,
    })
}
