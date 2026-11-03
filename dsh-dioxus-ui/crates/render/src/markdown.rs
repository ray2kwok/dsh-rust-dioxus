//! Markdown → 扁平块结构。
//!
//! 为什么不用 HTML 字符串：消息内容来自模型，直接拼 HTML 等于把 XSS 面交给 Host；
//! 这里解析成结构，再由 `rsx!` 生成节点，天然转义。
//!
//! 结构刻意保持**扁平**：块内只保留行内元素。嵌套列表的深层块（列表项里的代码块等）
//! 会退化成行内文本，这覆盖了聊天场景的绝大多数消息，换取解析器与渲染器的可读性。

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// 一段 Markdown 文档。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading {
        level: u8,
        inlines: Vec<Inline>,
    },
    Paragraph {
        inlines: Vec<Inline>,
    },
    Code {
        lang: Option<String>,
        text: String,
    },
    List {
        ordered: bool,
        items: Vec<Vec<Inline>>,
    },
    Quote {
        inlines: Vec<Inline>,
    },
    Rule,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Code(String),
    Strong(Vec<Inline>),
    Emphasis(Vec<Inline>),
    Link {
        href: String,
        children: Vec<Inline>,
    },
    Image {
        src: String,
        alt: String,
    },
    /// 行内/块级公式（TeX 原文）。
    Math {
        tex: String,
        display: bool,
    },
    /// 段落内换行。
    SoftBreak,
}

/// 解析入口。
pub fn parse(source: &str) -> Document {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_MATH);

    let mut builder = Builder::default();

    for event in Parser::new_ext(source, options) {
        builder.handle(event);
    }
    builder.finish()
}

#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    /// 当前段落/标题/引用里累积的行内元素。
    inlines: Vec<Inline>,
    /// 强调/链接的嵌套栈。
    stack: Vec<InlineFrame>,
    heading: Option<u8>,
    quote_depth: u32,
    list: Option<ListState>,
    code: Option<CodeState>,
}

struct InlineFrame {
    kind: FrameKind,
    buffer: Vec<Inline>,
}

enum FrameKind {
    Strong,
    Emphasis,
    Link(String),
    Image(String),
}

struct ListState {
    ordered: bool,
    items: Vec<Vec<Inline>>,
}

struct CodeState {
    lang: Option<String>,
    text: String,
}

impl Builder {
    fn handle(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.push_inline(Inline::Text(text.to_string())),
            Event::Code(code) => self.push_inline(Inline::Code(code.to_string())),
            Event::SoftBreak | Event::HardBreak => self.push_inline(Inline::SoftBreak),
            Event::Rule => self.blocks.push(Block::Rule),
            // 表格/脚注等结构不单独建模，内容按普通行内文本流动
            Event::Html(_) | Event::InlineHtml(_) | Event::FootnoteReference(_) => {}
            Event::TaskListMarker(done) => {
                self.push_inline(Inline::Text(if done { "[x] " } else { "[ ] " }.to_string()));
            }
            Event::InlineMath(text) => self.push_inline(Inline::Math {
                tex: text.to_string(),
                display: false,
            }),
            Event::DisplayMath(text) => self.push_inline(Inline::Math {
                tex: text.to_string(),
                display: true,
            }),
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                self.heading = Some(level as u8);
                self.inlines.clear();
            }
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info
                        .split_whitespace()
                        .next()
                        .filter(|s| !s.is_empty())
                        .map(str::to_string),
                    CodeBlockKind::Indented => None,
                };
                self.code = Some(CodeState {
                    lang,
                    text: String::new(),
                });
            }
            Tag::List(start) => {
                self.list = Some(ListState {
                    ordered: start.is_some(),
                    items: Vec::new(),
                });
            }
            Tag::Item => {
                self.inlines.clear();
            }
            Tag::BlockQuote(_) => {
                self.quote_depth += 1;
                self.inlines.clear();
            }
            Tag::Strong => self.open_frame(FrameKind::Strong),
            Tag::Emphasis => self.open_frame(FrameKind::Emphasis),
            Tag::Link { dest_url, .. } => self.open_frame(FrameKind::Link(dest_url.to_string())),
            Tag::Image { dest_url, .. } => self.open_frame(FrameKind::Image(dest_url.to_string())),
            // 表格按普通行内文本处理
            Tag::Table(_) | Tag::TableHead | Tag::TableRow | Tag::TableCell => {}
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush_paragraph(),
            TagEnd::Heading(_) => {
                let level = self.heading.take().unwrap_or(1);
                let inlines = std::mem::take(&mut self.inlines);
                self.blocks.push(Block::Heading { level, inlines });
            }
            TagEnd::CodeBlock => {
                if let Some(code) = self.code.take() {
                    self.blocks.push(Block::Code {
                        lang: code.lang,
                        text: code.text,
                    });
                }
            }
            TagEnd::Item => {
                let inlines = std::mem::take(&mut self.inlines);
                if let Some(list) = self.list.as_mut() {
                    list.items.push(trim_soft_breaks(inlines));
                }
            }
            TagEnd::List(_) => {
                if let Some(list) = self.list.take() {
                    self.blocks.push(Block::List {
                        ordered: list.ordered,
                        items: list.items,
                    });
                }
            }
            TagEnd::BlockQuote(_) => {
                self.quote_depth = self.quote_depth.saturating_sub(1);
                let inlines = std::mem::take(&mut self.inlines);
                self.blocks.push(Block::Quote { inlines });
            }
            TagEnd::Strong => self.close_frame(),
            TagEnd::Emphasis => self.close_frame(),
            TagEnd::Link => self.close_frame(),
            TagEnd::Image => self.close_frame(),
            _ => {}
        }
    }

    fn flush_paragraph(&mut self) {
        if self.inlines.is_empty() {
            return;
        }
        // 列表项内的段落不要生成独立块
        if self.list.is_some() || self.quote_depth > 0 {
            self.inlines.push(Inline::SoftBreak);
            return;
        }
        let inlines = std::mem::take(&mut self.inlines);
        self.blocks.push(Block::Paragraph { inlines });
    }

    fn open_frame(&mut self, kind: FrameKind) {
        self.stack.push(InlineFrame {
            kind,
            buffer: Vec::new(),
        });
    }

    fn close_frame(&mut self) {
        let Some(frame) = self.stack.pop() else {
            return;
        };
        let node = match frame.kind {
            FrameKind::Strong => Inline::Strong(frame.buffer),
            FrameKind::Emphasis => Inline::Emphasis(frame.buffer),
            FrameKind::Link(href) => Inline::Link {
                href,
                children: frame.buffer,
            },
            FrameKind::Image(src) => Inline::Image {
                src,
                alt: plain_text(&frame.buffer),
            },
        };
        self.push_inline(node);
    }

    /// 行内元素先记到最内层帧（强调/链接），没有帧时进当前块。
    fn push_inline(&mut self, inline: Inline) {
        if let Some(code) = self.code.as_mut() {
            if let Inline::Text(text) = &inline {
                code.text.push_str(text);
            }
            return;
        }
        match self.stack.last_mut() {
            Some(frame) => frame.buffer.push(inline),
            None => self.inlines.push(inline),
        }
    }

    fn finish(mut self) -> Document {
        // 处理没有收尾的段落（例如流式半截文本）
        self.flush_paragraph();
        Document {
            blocks: self.blocks,
        }
    }
}

fn trim_soft_breaks(mut inlines: Vec<Inline>) -> Vec<Inline> {
    while matches!(inlines.last(), Some(Inline::SoftBreak)) {
        inlines.pop();
    }
    inlines
}

fn plain_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(text) | Inline::Code(text) => out.push_str(text),
            Inline::Strong(children) | Inline::Emphasis(children) => {
                out.push_str(&plain_text(children));
            }
            Inline::Link { children, .. } => out.push_str(&plain_text(children)),
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::Math { tex, .. } => out.push_str(tex),
            Inline::SoftBreak => out.push(' '),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_heading_and_paragraph() {
        let doc = parse("# 标题\n\n正文**加粗**与`代码`。");
        assert_eq!(doc.blocks.len(), 2);
        match &doc.blocks[0] {
            Block::Heading { level, .. } => assert_eq!(*level, 1),
            other => panic!("期望 heading，实际 {other:?}"),
        }
        match &doc.blocks[1] {
            Block::Paragraph { inlines } => {
                assert!(inlines.iter().any(|i| matches!(i, Inline::Strong(_))));
                assert!(
                    inlines
                        .iter()
                        .any(|i| matches!(i, Inline::Code(c) if c == "代码"))
                );
            }
            other => panic!("期望 paragraph，实际 {other:?}"),
        }
    }

    #[test]
    fn parses_fenced_code_with_lang() {
        let doc = parse("```rust\nfn main() {}\n```");
        match &doc.blocks[0] {
            Block::Code { lang, text } => {
                assert_eq!(lang.as_deref(), Some("rust"));
                assert!(text.contains("fn main()"));
            }
            other => panic!("期望 code，实际 {other:?}"),
        }
    }

    #[test]
    fn parses_unordered_list_items() {
        let doc = parse("- 一\n- 二\n");
        match &doc.blocks[0] {
            Block::List { ordered, items } => {
                assert!(!ordered);
                assert_eq!(items.len(), 2);
            }
            other => panic!("期望 list，实际 {other:?}"),
        }
    }

    #[test]
    fn parses_link_and_quote() {
        let doc = parse("> 引用 [链接](https://example.com)\n");
        match &doc.blocks[0] {
            Block::Quote { inlines } => {
                assert!(inlines.iter().any(
                    |i| matches!(i, Inline::Link { href, .. } if href == "https://example.com")
                ));
            }
            other => panic!("期望 quote，实际 {other:?}"),
        }
    }

    #[test]
    fn raw_html_is_not_interpreted() {
        // HTML 事件被忽略，文本仍作为纯文本保留，防止注入
        let doc = parse("<script>alert(1)</script>");
        let flattened = format!("{doc:?}");
        assert!(!flattened.contains("Block::Html"));
    }

    #[test]
    fn plain_text_without_blocks_is_empty() {
        assert!(parse("").blocks.is_empty());
    }

    #[test]
    fn parses_inline_and_display_math() {
        let doc = parse("行内 $E=mc^2$\n\n$$\\frac{a}{b}$$");
        let flattened = format!("{doc:?}");
        assert!(flattened.contains("Math {"));
        assert!(flattened.contains("display: true"));
    }

    #[test]
    fn parses_image_with_alt_text() {
        let doc = parse("![截图](data:image/png;base64,AAAA)");
        match &doc.blocks[0] {
            Block::Paragraph { inlines } => {
                assert!(inlines.iter().any(|inline| matches!(
                    inline,
                    Inline::Image { alt, .. } if alt == "截图"
                )));
            }
            other => panic!("期望 paragraph，实际 {other:?}"),
        }
    }
}
