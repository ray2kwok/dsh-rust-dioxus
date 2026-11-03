//! 极简语法着色。
//!
//! 为什么不用 `syntect`：它带约 1.5 MB 的语法集与主题数据，而聊天里出现的代码
//! 绝大多数是常见语言的一小段片段。这里用约 100 行的手写扫描器覆盖
//! 「关键字 / 字符串 / 注释 / 数字」四类，零依赖、零体积增长。
//!
//! 输出的 class 名与 CSS 变量解耦，主题切换时不需要重新解析。
//! 后续若需要更完整的语言支持，可替换为 `syntect` 或 tree-sitter，
//! 只要保持 [`Span`] 这个契约不变，UI 层无需改动。

/// 语义类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Plain,
    Keyword,
    Str,
    Comment,
    Number,
    Function,
}

impl Token {
    pub fn css_class(self) -> &'static str {
        match self {
            Token::Plain => "tok",
            Token::Keyword => "tok tok--kw",
            Token::Str => "tok tok--str",
            Token::Comment => "tok tok--cmt",
            Token::Number => "tok tok--num",
            Token::Function => "tok tok--fn",
        }
    }
}

/// 一段带类别的文本片段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub token: Token,
    pub text: String,
}

/// 按行着色；调用方逐行渲染以便显示行号或做折叠。
pub fn highlight(code: &str, lang: Option<&str>) -> Vec<Vec<Span>> {
    let keywords = keywords_for(lang);
    code.lines().map(|line| scan_line(line, keywords)).collect()
}

fn keywords_for(lang: Option<&str>) -> &'static [&'static str] {
    match lang.unwrap_or_default().to_ascii_lowercase().as_str() {
        "rust" | "rs" => &[
            "fn", "let", "mut", "pub", "use", "impl", "struct", "enum", "match", "if", "else",
            "for", "while", "loop", "return", "self", "Self", "trait", "where", "async", "await",
            "move", "const", "static", "mod", "crate", "dyn", "ref", "in", "as", "unsafe",
        ],
        "typescript" | "ts" | "javascript" | "js" | "tsx" | "jsx" => &[
            "const",
            "let",
            "var",
            "function",
            "return",
            "if",
            "else",
            "for",
            "while",
            "class",
            "interface",
            "type",
            "import",
            "export",
            "from",
            "async",
            "await",
            "new",
            "this",
            "extends",
            "implements",
            "enum",
            "throw",
            "try",
            "catch",
            "finally",
            "switch",
            "case",
            "default",
            "null",
            "undefined",
            "true",
            "false",
        ],
        "python" | "py" => &[
            "def", "class", "import", "from", "as", "return", "if", "elif", "else", "for", "while",
            "with", "try", "except", "finally", "lambda", "None", "True", "False", "async",
            "await", "yield", "pass", "raise", "in", "is", "not", "and", "or",
        ],
        "bash" | "sh" | "shell" | "zsh" => &[
            "if", "then", "else", "elif", "fi", "for", "in", "do", "done", "while", "case", "esac",
            "function", "local", "export", "return", "set", "cd", "echo", "exec", "eval",
        ],
        "json" => &["true", "false", "null"],
        "toml" | "yaml" | "yml" => &["true", "false", "null"],
        _ => &[],
    }
}

fn scan_line(line: &str, keywords: &'static [&'static str]) -> Vec<Span> {
    let chars: Vec<char> = line.chars().collect();
    let mut spans: Vec<Span> = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        // 行注释：// 或 #
        if (ch == '/' && chars.get(i + 1) == Some(&'/')) || ch == '#' {
            push(&mut spans, Token::Comment, &chars[i..]);
            break;
        }

        // 块注释：/* ... */（同一行内闭合）
        if ch == '/' && chars.get(i + 1) == Some(&'*') {
            let mut end = i + 2;
            while end + 1 < chars.len() && !(chars[end] == '*' && chars[end + 1] == '/') {
                end += 1;
            }
            let stop = (end + 2).min(chars.len());
            push(&mut spans, Token::Comment, &chars[i..stop]);
            i = stop;
            continue;
        }

        // 字符串
        if ch == '"' || ch == '\'' || ch == '`' {
            let quote = ch;
            let mut end = i + 1;
            while end < chars.len() {
                if chars[end] == '\\' {
                    end += 2;
                    continue;
                }
                if chars[end] == quote {
                    end += 1;
                    break;
                }
                end += 1;
            }
            let stop = end.min(chars.len());
            push(&mut spans, Token::Str, &chars[i..stop]);
            i = stop;
            continue;
        }

        // 数字
        if ch.is_ascii_digit() {
            let mut end = i;
            while end < chars.len()
                && (chars[end].is_ascii_alphanumeric() || chars[end] == '.' || chars[end] == '_')
            {
                end += 1;
            }
            push(&mut spans, Token::Number, &chars[i..end]);
            i = end;
            continue;
        }

        // 标识符 / 关键字
        if ch.is_alphanumeric() || ch == '_' {
            let mut end = i;
            while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
                end += 1;
            }
            let word: String = chars[i..end].iter().collect();
            let token = if keywords.contains(&word.as_str()) {
                Token::Keyword
            } else if chars.get(end) == Some(&'(') {
                Token::Function
            } else {
                Token::Plain
            };
            push(&mut spans, token, &chars[i..end]);
            i = end;
            continue;
        }

        // 其余字符
        push(&mut spans, Token::Plain, &chars[i..=i]);
        i += 1;
    }

    spans
}

fn push(spans: &mut Vec<Span>, token: Token, chars: &[char]) {
    let text: String = chars.iter().collect();
    if text.is_empty() {
        return;
    }
    // 合并相邻同类片段，减少节点数
    if let Some(last) = spans.last_mut()
        && last.token == token
    {
        last.text.push_str(&text);
        return;
    }
    spans.push(Span { token, text });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes(code: &str, lang: &str) -> Vec<Token> {
        highlight(code, Some(lang))
            .into_iter()
            .flatten()
            .map(|span| span.token)
            .collect()
    }

    #[test]
    fn detects_keyword_string_comment() {
        let tokens = classes("let x = \"hi\"; // 注释", "rust");
        assert!(tokens.contains(&Token::Keyword));
        assert!(tokens.contains(&Token::Str));
        assert!(tokens.contains(&Token::Comment));
    }

    #[test]
    fn detects_numbers() {
        let tokens = classes("let n = 42;", "rust");
        assert!(tokens.contains(&Token::Number));
    }

    #[test]
    fn function_names_are_detected() {
        let flat = highlight("fn main() {}", Some("rust"));
        assert!(
            flat[0]
                .iter()
                .any(|s| s.token == Token::Function && s.text == "main"),
            "{:?}",
            flat
        );
    }

    #[test]
    fn unknown_language_degrades_to_plain() {
        let tokens = classes("SELECT * FROM t", "sql-not-supported");
        assert!(tokens.iter().all(|t| *t == Token::Plain));
    }

    #[test]
    fn keeps_line_structure() {
        let lines = highlight("a\nb\nc", None);
        assert_eq!(lines.len(), 3);
    }
}
