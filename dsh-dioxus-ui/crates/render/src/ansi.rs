//! 终端输出的最小 ANSI 解析。
//!
//! 目标不是完整的终端模拟器，而是把 Host 下发的屏幕文本变成可渲染的
//! 「行 → 带样式的片段」：
//!
//! - 支持 SGR（粗体/暗色/斜体/下划线/反显、16 色 / 256 色 / 真彩色）；
//! - 支持 `\r` 覆盖当前行（进度条）、`\t` 对齐、`\x08` 退格；
//! - 其余 CSI / OSC / 单字符转义一律丢弃，光标移动不做建模（快照已按行给出）。
//!
//! 纯函数、零依赖：`dsh-render` 可在 `cargo test` 里直接断言解析结果。

/// 终端颜色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnsiColor {
    #[default]
    Default,
    /// 256 色索引（0–15 是基础色，16–231 是 6×6×6 色立方，232–255 是灰阶）。
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl AnsiColor {
    pub fn css(self) -> Option<String> {
        match self {
            Self::Default => None,
            Self::Indexed(index) => Some(indexed_css(index)),
            Self::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
        }
    }
}

/// 一个片段的显示样式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnsiStyle {
    pub fg: AnsiColor,
    pub bg: AnsiColor,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    /// 反显：渲染时交换前景/背景。
    pub reverse: bool,
}

impl AnsiStyle {
    /// 展开成内联 CSS；空串表示默认样式。
    pub fn css(&self) -> String {
        let (fg, bg) = if self.reverse {
            (self.bg, self.fg)
        } else {
            (self.fg, self.bg)
        };

        let mut css = String::new();
        if let Some(color) = fg.css() {
            css.push_str("color:");
            css.push_str(&color);
            css.push(';');
        }
        if let Some(color) = bg.css() {
            css.push_str("background-color:");
            css.push_str(&color);
            css.push(';');
        }
        if self.bold {
            // WebView 的等宽字体通常没有 700 字重，600 更稳
            css.push_str("font-weight:600;");
        }
        if self.dim {
            css.push_str("opacity:.65;");
        }
        if self.italic {
            css.push_str("font-style:italic;");
        }
        if self.underline {
            css.push_str("text-decoration:underline;");
        }
        css
    }
}

/// 一行里的一个同样式片段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnsiSpan {
    pub text: String,
    pub style: AnsiStyle,
}

/// 解析后的一行。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnsiLine {
    pub spans: Vec<AnsiSpan>,
}

impl AnsiLine {
    /// 纯文本（不含样式），用于复制/检索。
    pub fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.iter().all(|span| span.text.is_empty())
    }

    fn trim_end(&mut self) {
        for span in self.spans.iter_mut().rev() {
            let trimmed = span.text.trim_end_matches([' ', '\t']).len();
            span.text.truncate(trimmed);
        }
        self.spans.retain(|span| !span.text.is_empty());
    }
}

/// 把带 ANSI 转义序列的终端文本解析成行。
pub fn parse(input: &str) -> Vec<AnsiLine> {
    let mut lines: Vec<AnsiLine> = Vec::new();
    let mut spans: Vec<AnsiSpan> = Vec::new();
    let mut text = String::new();
    let mut style = AnsiStyle::default();
    let mut pending_cr = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\x1b' => {
                escape(&mut chars, &mut style, &mut spans, &mut text);
            }
            '\n' => {
                pending_cr = false;
                push_line(&mut lines, &mut spans, &mut text, style);
            }
            '\r' => pending_cr = true,
            '\t' => {
                let width = line_width(&spans, &text);
                let pad = 8 - (width % 8);
                if pending_cr {
                    spans.clear();
                    text.clear();
                    pending_cr = false;
                }
                text.extend(std::iter::repeat_n(' ', pad));
            }
            '\x08' => {
                // 退格只回退当前缓冲区的一个字符（不跨片段）
                text.pop();
            }
            ch if (ch as u32) < 0x20 || ch == '\x7f' => {}
            ch => {
                if pending_cr {
                    // 回到行首后的输出覆盖整行（进度条语义）
                    spans.clear();
                    text.clear();
                    pending_cr = false;
                }
                text.push(ch);
            }
        }
    }

    if !text.is_empty() || !spans.is_empty() {
        push_line(&mut lines, &mut spans, &mut text, style);
    } else if lines.is_empty() {
        lines.push(AnsiLine::default());
    }
    lines
}

fn push_line(
    lines: &mut Vec<AnsiLine>,
    spans: &mut Vec<AnsiSpan>,
    text: &mut String,
    style: AnsiStyle,
) {
    flush_span(spans, text, style);
    let mut line = AnsiLine {
        spans: std::mem::take(spans),
    };
    line.trim_end();
    lines.push(line);
}

fn flush_span(spans: &mut Vec<AnsiSpan>, text: &mut String, style: AnsiStyle) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = spans.last_mut()
        && last.style == style
    {
        last.text.push_str(text);
        text.clear();
        return;
    }
    spans.push(AnsiSpan {
        text: std::mem::take(text),
        style,
    });
}

/// 处理 `ESC` 之后的序列；无法识别的一律吞掉。
fn escape(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    style: &mut AnsiStyle,
    spans: &mut Vec<AnsiSpan>,
    text: &mut String,
) {
    match chars.peek().copied() {
        Some('[') => {
            chars.next();
            let mut params = String::new();
            let mut final_byte = None;
            for ch in chars.by_ref() {
                if ('\x40'..='\x7e').contains(&ch) {
                    final_byte = Some(ch);
                    break;
                }
                if params.len() < 64 {
                    params.push(ch);
                }
            }
            if final_byte == Some('m') {
                // 样式切换前先封存上一段，否则颜色会「回溯」污染已输出的文本
                flush_span(spans, text, *style);
                apply_sgr(style, &params);
            }
            // 其余 CSI（光标移动 / 擦除）不建模：快照已按行给出，
            // 擦除留下的尾部空格也会在行末被裁掉。
        }
        Some(']') => {
            chars.next();
            // OSC（标题、超链接等）：吞到 BEL 或 ST
            while let Some(ch) = chars.next() {
                if ch == '\x07' {
                    break;
                }
                if ch == '\x1b' && chars.peek() == Some(&'\\') {
                    chars.next();
                    break;
                }
            }
        }
        Some(_) => {
            chars.next();
        }
        None => {}
    }
}

/// 应用 SGR 参数。
fn apply_sgr(style: &mut AnsiStyle, params: &str) {
    let values: Vec<u16> = params
        .split(';')
        .map(|part| {
            if part.is_empty() {
                0
            } else {
                part.parse::<u16>().unwrap_or(0)
            }
        })
        .collect();

    let mut index = 0;
    while index < values.len() {
        match values[index] {
            0 => *style = AnsiStyle::default(),
            1 => style.bold = true,
            2 => style.dim = true,
            3 => style.italic = true,
            4 => style.underline = true,
            7 => style.reverse = true,
            22 => {
                style.bold = false;
                style.dim = false;
            }
            23 => style.italic = false,
            24 => style.underline = false,
            27 => style.reverse = false,
            30..=37 => style.fg = AnsiColor::Indexed((values[index] - 30) as u8),
            39 => style.fg = AnsiColor::Default,
            40..=47 => style.bg = AnsiColor::Indexed((values[index] - 40) as u8),
            49 => style.bg = AnsiColor::Default,
            90..=97 => style.fg = AnsiColor::Indexed((values[index] - 90 + 8) as u8),
            100..=107 => style.bg = AnsiColor::Indexed((values[index] - 100 + 8) as u8),
            38 | 48 => {
                let foreground = values[index] == 38;
                match values.get(index + 1).copied() {
                    Some(5) => {
                        if let Some(value) = values.get(index + 2) {
                            let color = AnsiColor::Indexed(*value as u8);
                            if foreground {
                                style.fg = color;
                            } else {
                                style.bg = color;
                            }
                            index += 2;
                        }
                    }
                    Some(2) => {
                        if let (Some(r), Some(g), Some(b)) = (
                            values.get(index + 2),
                            values.get(index + 3),
                            values.get(index + 4),
                        ) {
                            let color = AnsiColor::Rgb(*r as u8, *g as u8, *b as u8);
                            if foreground {
                                style.fg = color;
                            } else {
                                style.bg = color;
                            }
                            index += 4;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        index += 1;
    }
}

/// 基础 16 色 + 6×6×6 色立方 + 灰阶（以深色底可读为准则）。
fn indexed_css(index: u8) -> String {
    const BASE: [&str; 16] = [
        "#3b4048", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#dcdfe4",
        "#7f848e", "#ff7b86", "#b5e890", "#ffd68a", "#7cc4ff", "#d89cff", "#6fd4e0", "#f2f4f8",
    ];
    const CUBE: [u8; 6] = [0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff];

    match index {
        0..=15 => BASE[index as usize].to_string(),
        16..=231 => {
            let value = index - 16;
            let r = CUBE[(value / 36) as usize];
            let g = CUBE[((value % 36) / 6) as usize];
            let b = CUBE[(value % 6) as usize];
            format!("#{r:02x}{g:02x}{b:02x}")
        }
        232..=255 => {
            let level = 8 + (index - 232) * 10;
            format!("#{level:02x}{level:02x}{level:02x}")
        }
    }
}

/// 估算显示宽度：CJK / 全角字符按 2 列计。
fn line_width(spans: &[AnsiSpan], text: &str) -> usize {
    let mut width = 0;
    for span in spans {
        width += span.text.chars().map(char_width).sum::<usize>();
    }
    width + text.chars().map(char_width).sum::<usize>()
}

fn char_width(ch: char) -> usize {
    let code = ch as u32;
    let wide = (0x1100..=0x115f).contains(&code)
        || (0x2e80..=0xa4cf).contains(&code)
        || (0xac00..=0xd7a3).contains(&code)
        || (0xf900..=0xfaff).contains(&code)
        || (0xfe30..=0xfe6f).contains(&code)
        || (0xff00..=0xff60).contains(&code)
        || (0xffe0..=0xffe6).contains(&code)
        || (0x1f300..=0x1faff).contains(&code);
    if wide { 2 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: &[AnsiLine]) -> Vec<String> {
        lines.iter().map(AnsiLine::text).collect()
    }

    #[test]
    fn plain_text_splits_lines() {
        let lines = parse("one\ntwo\n");
        assert_eq!(texts(&lines), vec!["one", "two"]);
    }

    #[test]
    fn sgr_color_and_reset() {
        let lines = parse("\x1b[31mred\x1b[0m plain");
        assert_eq!(lines[0].spans.len(), 2);
        assert_eq!(lines[0].spans[0].text, "red");
        assert_eq!(lines[0].spans[0].style.fg, AnsiColor::Indexed(1));
        assert!(lines[0].spans[0].style.css().contains("color:#e06c75"));
        assert_eq!(lines[0].spans[1].style, AnsiStyle::default());
    }

    #[test]
    fn bright_and_256_colors() {
        let lines = parse("\x1b[91mA\x1b[38;5;196mB\x1b[38;2;1;2;3mC");
        assert_eq!(lines[0].spans[0].style.fg, AnsiColor::Indexed(9));
        assert_eq!(lines[0].spans[1].style.fg, AnsiColor::Indexed(196));
        assert_eq!(lines[0].spans[2].style.fg, AnsiColor::Rgb(1, 2, 3));
        assert!(lines[0].spans[2].style.css().contains("color:#010203"));
    }

    #[test]
    fn reverse_swaps_colors() {
        let lines = parse("\x1b[31;47;7mX");
        let css = lines[0].spans[0].style.css();
        assert!(css.contains("color:#dcdfe4"), "{css}");
        assert!(css.contains("background-color:#e06c75"), "{css}");
    }

    #[test]
    fn cursor_moves_and_osc_are_dropped() {
        let lines = parse("\x1b]0;title\x07\x1b[1;1Hhello\x1b[K");
        assert_eq!(texts(&lines), vec!["hello"]);
    }

    #[test]
    fn carriage_return_overwrites_line() {
        let lines = parse("progress 10%\rprogress 100%");
        assert_eq!(texts(&lines), vec!["progress 100%"]);
    }

    #[test]
    fn tab_aligns_to_eight_columns() {
        let lines = parse("ab\tc");
        assert_eq!(texts(&lines), vec!["ab      c"]);
    }

    #[test]
    fn trailing_spaces_are_trimmed_but_empty_line_survives() {
        let lines = parse("a   \n\nb");
        assert_eq!(texts(&lines), vec!["a", "", "b"]);
    }

    #[test]
    fn backspace_removes_last_char() {
        let lines = parse("abc\x08d");
        assert_eq!(texts(&lines), vec!["abd"]);
    }
}
