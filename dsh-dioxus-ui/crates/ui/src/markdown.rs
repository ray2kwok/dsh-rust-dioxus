//! Markdown 渲染组件。
//!
//! 解析在 `dsh-render`（纯函数、可单测），这里只负责把块结构转成 `rsx!` 节点。
//! 全流程不拼接 HTML 字符串，因此不需要 `dangerous_inner_html`，也不会有注入面。

use dioxus::prelude::*;
use dsh_render::{Block, Inline};

/// 渲染一段 Markdown。
#[component]
pub fn MarkdownView(source: String) -> Element {
    let document = dsh_render::parse_markdown(&source);

    rsx! {
        div { class: "md",
            for block in document.blocks {
                {render_block(&block)}
            }
        }
    }
}

fn render_block(block: &Block) -> Element {
    match block {
        Block::Heading { level, inlines } => {
            // 聊天场景里 h1 太抢眼，整体降一级
            let inlines = inlines.clone();
            match level {
                1 => rsx! { h3 { class: "md-h", {render_inlines(&inlines)} } },
                2 => rsx! { h4 { class: "md-h", {render_inlines(&inlines)} } },
                _ => rsx! { h5 { class: "md-h", {render_inlines(&inlines)} } },
            }
        }
        Block::Paragraph { inlines } => rsx! { p { {render_inlines(inlines)} } },
        Block::Code { lang, text } => {
            let label = lang.clone().unwrap_or_default();
            let lines = dsh_render::highlight(text, lang.as_deref());
            rsx! {
                div { class: "code-block",
                    div { class: "code-block__head",
                        span { class: "code-block__lang", "{label}" }
                    }
                    pre { class: "code-block__body",
                        code {
                            for (index, line) in lines.iter().enumerate() {
                                if index > 0 { "\n" }
                                for span in line {
                                    span { class: "{span.token.css_class()}", "{span.text}" }
                                }
                            }
                        }
                    }
                }
            }
        }
        Block::List { ordered, items } => {
            let items = items.clone();
            let ordered = *ordered;
            rsx! {
                if ordered {
                    ol { class: "md-list",
                        for item in items {
                            li { {render_inlines(&item)} }
                        }
                    }
                } else {
                    ul { class: "md-list",
                        for item in items {
                            li { {render_inlines(&item)} }
                        }
                    }
                }
            }
        }
        Block::Quote { inlines } => {
            rsx! { blockquote { class: "md-quote", {render_inlines(inlines)} } }
        }
        Block::Rule => rsx! { hr { class: "md-rule" } },
    }
}

fn render_inlines(inlines: &[Inline]) -> Element {
    rsx! {
        for inline in inlines {
            {render_inline(inline)}
        }
    }
}

fn render_inline(inline: &Inline) -> Element {
    match inline {
        Inline::Text(text) => rsx! { "{text}" },
        Inline::Code(code) => rsx! { code { class: "inline-code", "{code}" } },
        Inline::Strong(children) => {
            let children = children.clone();
            rsx! { strong { {render_inlines(&children)} } }
        }
        Inline::Emphasis(children) => {
            let children = children.clone();
            rsx! { em { {render_inlines(&children)} } }
        }
        Inline::Link { href, children } => {
            let href = href.clone();
            let children = children.clone();
            rsx! {
                a { class: "md-link", href: "{href}", target: "_blank", rel: "noreferrer",
                    {render_inlines(&children)}
                }
            }
        }
        Inline::SoftBreak => rsx! { br {} },
    }
}
