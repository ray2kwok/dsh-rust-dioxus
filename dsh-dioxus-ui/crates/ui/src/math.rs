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
}
