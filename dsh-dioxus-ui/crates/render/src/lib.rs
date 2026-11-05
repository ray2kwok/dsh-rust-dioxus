//! 内容渲染的纯逻辑层：Markdown 与 diff 的解析。
//!
//! 只产出中间结构，不依赖 Dioxus —— 因此可以在 `cargo test` 里对解析结果做断言，
//! 而不必启动窗口或浏览器。渲染（`rsx!`）在 `dsh-ui` 里完成。
//!
//! 设计取向：
//! - **不生成 HTML 字符串**：消息来自模型，拼 HTML 等于把 XSS 面交给对端；
//! - **结构扁平**：块内只保留行内元素，牺牲极少见的深层嵌套换取可读性；
//! - **降级优先**：任何无法识别的结构都退化成文本，不 panic。

pub mod ansi;
pub mod diff;
pub mod highlight;
pub mod markdown;

pub use ansi::{AnsiColor, AnsiLine, AnsiSpan, AnsiStyle, parse as parse_ansi};
pub use diff::{DiffKind, DiffRow, parse_unified, rows as diff_rows, stats as diff_stats};
pub use highlight::{Span, Token, highlight};
pub use markdown::{Block, Document, Inline, ListItem, parse as parse_markdown};
