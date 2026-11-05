//! 轻量数学公式渲染。
//!
//! 不引入 KaTeX 运行时：常见公式（希腊字母、上下标、分数、根号）转成
//! Unicode + `<sup>/<sub>` + CSS 分数布局；无法识别的命令保留原文，
//! 保证公式至少可见、可复制。

use dioxus::prelude::*;

#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Text(String),
    Sup(String),
    Sub(String),
    Frac { num: String, den: String },
    Sqrt(String),
}

/// 渲染一段 TeX（`display` 时使用块级样式）。
#[component]
pub fn MathView(tex: String, #[props(default = false)] display: bool) -> Element {
    let pieces = parse(&tex);
    rsx! {
        div {
            class: if display { "math math--display" } else { "math math--inline" },
            for (index, piece) in pieces.iter().enumerate() {
                {
                    match piece {
                        Piece::Text(text) => rsx! { span { key: "{index}", "{text}" } },
                        Piece::Sup(text) => rsx! { sup { key: "{index}", "{text}" } },
                        Piece::Sub(text) => rsx! { sub { key: "{index}", "{text}" } },
                        Piece::Frac { num, den } => rsx! {
                            span { key: "{index}", class: "math-frac",
                                span { class: "math-frac__num", "{num}" }
                                span { class: "math-frac__den", "{den}" }
                            }
                        },
                        Piece::Sqrt(value) => rsx! {
                            span { key: "{index}", "√({value})" }
                        },
                    }
                }
            }
        }
    }
}

fn parse(tex: &str) -> Vec<Piece> {
    let chars: Vec<char> = tex.chars().collect();
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut index = 0;

    let flush = |pieces: &mut Vec<Piece>, text: &mut String| {
        if !text.is_empty() {
            pieces.push(Piece::Text(std::mem::take(text)));
        }
    };

    while index < chars.len() {
        let ch = chars[index];
        if ch == '\\' {
            let (command, next) = read_command(&chars, index + 1);
            index = next;
            match command.as_str() {
                "frac" => {
                    let (num, next) = read_group(&chars, index);
                    let (den, next) = read_group(&chars, next);
                    flush(&mut pieces, &mut text);
                    pieces.push(Piece::Frac {
                        num: simple_text(&parse(&num)),
                        den: simple_text(&parse(&den)),
                    });
                    index = next;
                }
                "sqrt" => {
                    let (value, next) = read_group(&chars, index);
                    flush(&mut pieces, &mut text);
                    pieces.push(Piece::Sqrt(simple_text(&parse(&value))));
                    index = next;
                }
                "text" | "mathrm" | "operatorname" => {
                    let (value, next) = read_group(&chars, index);
                    text.push_str(&value);
                    index = next;
                }
                // 间距命令：\, \; \: 常见于公式里的分隔，不能原样显示成 "\,"。
                "," => text.push('\u{2009}'),
                ":" | " " => text.push(' '),
                ";" => text.push('\u{2002}'),
                "quad" => text.push_str("  "),
                "qquad" => text.push_str("    "),
                "!" => {}
                other => {
                    if let Some(value) = symbol(other) {
                        text.push_str(value);
                    } else {
                        text.push('\\');
                        text.push_str(other);
                    }
                }
            }
            continue;
        }

        if ch == '^' || ch == '_' {
            let (value, next) = read_group(&chars, index + 1);
            flush(&mut pieces, &mut text);
            if ch == '^' {
                pieces.push(Piece::Sup(simple_text(&parse(&value))));
            } else {
                pieces.push(Piece::Sub(simple_text(&parse(&value))));
            }
            index = next;
            continue;
        }

        if ch.is_whitespace() {
            if !text.ends_with(' ') {
                text.push(' ');
            }
            index += 1;
            continue;
        }

        text.push(ch);
        index += 1;
    }
    flush(&mut pieces, &mut text);
    pieces
}

fn read_command(chars: &[char], mut index: usize) -> (String, usize) {
    let start = index;
    while index < chars.len() && chars[index].is_ascii_alphabetic() {
        index += 1;
    }
    if start == index {
        return (
            chars.get(index).copied().unwrap_or(' ').to_string(),
            index + 1,
        );
    }
    (chars[start..index].iter().collect(), index)
}

fn read_group(chars: &[char], mut index: usize) -> (String, usize) {
    while index < chars.len() && chars[index].is_whitespace() {
        index += 1;
    }
    if index >= chars.len() {
        return (String::new(), index);
    }
    if chars[index] != '{' {
        return (chars[index].to_string(), index + 1);
    }
    let start = index + 1;
    let mut depth = 1;
    index += 1;
    while index < chars.len() {
        match chars[index] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return (chars[start..index].iter().collect(), index + 1);
                }
            }
            _ => {}
        }
        index += 1;
    }
    (chars[start..].iter().collect(), index)
}

fn simple_text(pieces: &[Piece]) -> String {
    pieces
        .iter()
        .map(|piece| match piece {
            Piece::Text(text) | Piece::Sup(text) | Piece::Sub(text) | Piece::Sqrt(text) => {
                text.clone()
            }
            Piece::Frac { num, den } => format!("{num}/{den}"),
        })
        .collect()
}

fn symbol(command: &str) -> Option<&'static str> {
    Some(match command {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" => "ε",
        "theta" => "θ",
        "lambda" => "λ",
        "mu" => "μ",
        "pi" => "π",
        "rho" => "ρ",
        "sigma" => "σ",
        "tau" => "τ",
        "phi" => "φ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Phi" => "Φ",
        "Omega" => "Ω",
        "times" => "×",
        "cdot" => "·",
        "pm" => "±",
        "le" | "leq" => "≤",
        "ge" | "geq" => "≥",
        "neq" => "≠",
        "approx" => "≈",
        "infty" => "∞",
        "sum" => "∑",
        "prod" => "∏",
        "int" => "∫",
        "partial" => "∂",
        "nabla" => "∇",
        "to" | "rightarrow" => "→",
        "leftarrow" => "←",
        "Rightarrow" => "⇒",
        "in" => "∈",
        "notin" => "∉",
        "subset" => "⊂",
        "forall" => "∀",
        "exists" => "∃",
        "ldots" | "dots" => "…",
        "quad" | "qquad" => " ",
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "log" => "log",
        "ln" => "ln",
        "max" => "max",
        "min" => "min",
        _ => return None,
    })
}

/// 一段纯文本里的数学片段：`$…$` 行内、`$$…$$` 块级，其余原样保留。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MathChunk {
    Text(String),
    Inline(String),
    Display(String),
}

/// 按 `$`/`$$` 切分纯文本（用户输入不解析 Markdown，但公式要认）。
pub fn split_math(text: &str) -> Vec<MathChunk> {
    let chars: Vec<char> = text.chars().collect();
    let mut chunks = Vec::new();
    let mut buffer = String::new();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] != '$' {
            buffer.push(chars[index]);
            index += 1;
            continue;
        }
        let display = index + 1 < chars.len() && chars[index + 1] == '$';
        let width = if display { 2 } else { 1 };
        let start = index + width;
        match find_close(&chars, start, width) {
            Some(end) if end > start => {
                if !buffer.is_empty() {
                    chunks.push(MathChunk::Text(std::mem::take(&mut buffer)));
                }
                let tex: String = chars[start..end].iter().collect();
                chunks.push(if display {
                    MathChunk::Display(tex)
                } else {
                    MathChunk::Inline(tex)
                });
                index = end + width;
            }
            _ => {
                buffer.push(chars[index]);
                index += 1;
            }
        }
    }
    if !buffer.is_empty() {
        chunks.push(MathChunk::Text(buffer));
    }
    chunks
}

fn find_close(chars: &[char], mut index: usize, width: usize) -> Option<usize> {
    while index < chars.len() {
        if chars[index] == '$' {
            if width == 1 {
                return Some(index);
            }
            if chars.get(index + 1) == Some(&'$') {
                return Some(index);
            }
        }
        index += 1;
    }
    None
}

/// 用户消息：纯文本 + 公式。
#[component]
pub fn MathText(text: String) -> Element {
    let chunks = split_math(&text);
    rsx! {
        for (index, chunk) in chunks.iter().enumerate() {
            {
                match chunk {
                    MathChunk::Text(value) => rsx! { span { key: "{index}", "{value}" } },
                    MathChunk::Inline(tex) => rsx! {
                        MathView { key: "{index}", tex: tex.clone(), display: false }
                    },
                    MathChunk::Display(tex) => rsx! {
                        MathView { key: "{index}", tex: tex.clone(), display: true }
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scripts_and_symbols() {
        let pieces = parse(r"x^2 + \alpha_1");
        assert!(matches!(pieces[1], Piece::Sup(ref value) if value == "2"));
        assert!(
            pieces
                .iter()
                .any(|piece| matches!(piece, Piece::Text(text) if text.contains('α')))
        );
        assert!(
            pieces
                .iter()
                .any(|piece| matches!(piece, Piece::Sub(value) if value == "1"))
        );
    }

    #[test]
    fn parses_fraction_and_sqrt() {
        let pieces = parse(r"\frac{a+b}{2}");
        assert!(
            matches!(pieces[0], Piece::Frac { ref num, ref den } if num == "a+b" && den == "2")
        );
        let pieces = parse(r"\sqrt{x+1}");
        assert!(matches!(pieces[0], Piece::Sqrt(ref value) if value == "x+1"));
    }

    #[test]
    fn unknown_command_keeps_source() {
        let pieces = parse(r"\unknown{x}");
        let text = simple_text(&pieces);
        assert!(text.contains("\\unknown"));
    }

    #[test]
    fn splits_inline_and_display_math() {
        let chunks = split_math("前 $a^2$ 后 $$\\int_0^1$$ 尾");
        assert_eq!(
            chunks,
            vec![
                MathChunk::Text("前 ".into()),
                MathChunk::Inline("a^2".into()),
                MathChunk::Text(" 后 ".into()),
                MathChunk::Display("\\int_0^1".into()),
                MathChunk::Text(" 尾".into()),
            ]
        );
    }

    #[test]
    fn unmatched_dollar_stays_text() {
        let chunks = split_math("价格 $5 元");
        assert_eq!(chunks, vec![MathChunk::Text("价格 $5 元".into())]);
    }

    #[test]
    fn empty_formula_stays_text() {
        assert_eq!(split_math("$$"), vec![MathChunk::Text("$$".into())]);
    }

    #[test]
    fn spacing_commands_do_not_leak_backslashes() {
        let text = simple_text(&parse(r"\int_0^1 x\,dx"));
        assert!(text.contains('∫'));
        assert!(!text.contains('\\'), "间距命令不应原样显示：{text}");
    }
}
