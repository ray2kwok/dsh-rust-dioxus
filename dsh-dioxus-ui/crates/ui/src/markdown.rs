//! Markdown 渲染组件。
//!
//! 解析在 `dsh-render`（纯函数、可单测），这里只负责把块结构转成 `rsx!` 节点。
//! 全流程不拼接 HTML 字符串，因此不需要 `dangerous_inner_html`，也不会有注入面。

use dioxus::prelude::*;
use dsh_render::{Block, Inline};

use crate::math::MathView;

/// 渲染一段 Markdown。
#[component]
pub fn MarkdownView(
    source: String,
    #[props(default = EventHandler::new(|_| {}))] on_file_ref: EventHandler<String>,
) -> Element {
    use_context_provider(|| on_file_ref);
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
                        for (index, item) in items.iter().enumerate() {
                            li {
                                key: "{index}",
                                class: "md-li",
                                style: "margin-left: {item.depth * 18}px",
                                {render_inlines(&item.inlines)}
                            }
                        }
                    }
                } else {
                    ul { class: "md-list",
                        for (index, item) in items.iter().enumerate() {
                            li {
                                key: "{index}",
                                class: "md-li",
                                style: "margin-left: {item.depth * 18}px",
                                {render_inlines(&item.inlines)}
                            }
                        }
                    }
                }
            }
        }
        Block::Table { head, rows } => {
            let head = head.clone();
            let rows = rows.clone();
            rsx! {
                div { class: "md-table-wrap",
                    table { class: "md-table",
                        if !head.is_empty() {
                            thead {
                                tr {
                                    for (index, cell) in head.iter().enumerate() {
                                        th { key: "{index}", {render_inlines(cell)} }
                                    }
                                }
                            }
                        }
                        tbody {
                            for (row_index, row) in rows.iter().enumerate() {
                                tr { key: "{row_index}",
                                    for (cell_index, cell) in row.iter().enumerate() {
                                        td { key: "{cell_index}", {render_inlines(cell)} }
                                    }
                                }
                            }
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
        Inline::Text(text) => render_text_with_refs(text),
        Inline::Code(code) => rsx! { code { class: "inline-code", "{code}" } },
        Inline::Strong(children) => {
            let children = children.clone();
            rsx! { strong { {render_inlines(&children)} } }
        }
        Inline::Emphasis(children) => {
            let children = children.clone();
            rsx! { em { {render_inlines(&children)} } }
        }
        Inline::Strike(children) => {
            let children = children.clone();
            rsx! { del { {render_inlines(&children)} } }
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
        Inline::Image { src, alt } => {
            let safe = src.starts_with("data:image/")
                || src.starts_with("https://")
                || src.starts_with("http://");
            if safe {
                rsx! { img { class: "md-image", src: "{src}", alt: "{alt}" } }
            } else {
                rsx! { span { class: "muted", "[image: {alt}]" } }
            }
        }
        Inline::Math { tex, display } => rsx! {
            MathView { tex: tex.clone(), display: *display }
        },
        Inline::SoftBreak => rsx! { br {} },
    }
}

/// 把文本里的 `src/main.rs:24-30` / `src/main.rs#L24` 变成可点击的文件引用。
fn render_text_with_refs(text: &str) -> Element {
    let Some(on_file_ref) = try_use_context::<EventHandler<String>>() else {
        return rsx! { "{text}" };
    };
    let mut segments: Vec<(String, Option<String>)> = Vec::new();
    let mut cursor = 0;
    for (start, end) in word_ranges(text) {
        let token = &text[start..end];
        let Some(target) = file_ref_target(token) else {
            continue;
        };
        if start > cursor {
            segments.push((text[cursor..start].to_string(), None));
        }
        segments.push((token.to_string(), Some(target)));
        cursor = end;
    }
    if cursor == 0 {
        return rsx! { "{text}" };
    }
    if cursor < text.len() {
        segments.push((text[cursor..].to_string(), None));
    }

    rsx! {
        for (index, (piece, target)) in segments.into_iter().enumerate() {
            if let Some(target) = target {
                button {
                    key: "{index}",
                    class: "file-ref",
                    title: "{target}",
                    onclick: move |_| on_file_ref.call(target.clone()),
                    "{piece}"
                }
            } else {
                span { key: "{index}", "{piece}" }
            }
        }
    }
}

fn word_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = None;
    for (index, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if let Some(from) = start.take() {
                ranges.push((from, index));
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(from) = start {
        ranges.push((from, text.len()));
    }
    ranges
}

fn file_ref_target(token: &str) -> Option<String> {
    let token = token.trim_end_matches([',', '.', ';', ')', ']', '，', '。']);
    let marker = token.find("#L").or_else(|| {
        token.rfind(':').filter(|index| {
            token[*index + 1..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_digit())
        })
    })?;
    let path = token[..marker].trim();
    if path.is_empty() || (!path.contains('/') && !path.contains('.')) {
        return None;
    }
    Some(path.to_string())
}
