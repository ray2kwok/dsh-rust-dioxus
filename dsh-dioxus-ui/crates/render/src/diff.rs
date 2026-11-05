//! 文本 diff → 可渲染的行序列。
//!
//! 对应上游 `ui-primitives/src/DiffBlock.tsx` 的数据侧：行号 + 变更类型 + 文本。
//! 行内高亮（同一行内的字符级 diff）留到后续版本。

use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffRow {
    pub kind: DiffKind,
    pub old_no: Option<usize>,
    pub new_no: Option<usize>,
    pub text: String,
}

impl DiffRow {
    pub fn css_class(&self) -> &'static str {
        match self.kind {
            DiffKind::Context => "diff-row",
            DiffKind::Added => "diff-row diff-row--add",
            DiffKind::Removed => "diff-row diff-row--del",
        }
    }

    pub fn marker(&self) -> &'static str {
        match self.kind {
            DiffKind::Context => " ",
            DiffKind::Added => "+",
            DiffKind::Removed => "-",
        }
    }
}

/// 计算两段文本的行级 diff。
pub fn rows(old: &str, new: &str) -> Vec<DiffRow> {
    let diff = TextDiff::from_lines(old, new);
    let mut rows = Vec::new();
    let mut old_no = 0usize;
    let mut new_no = 0usize;

    for change in diff.iter_all_changes() {
        let text = change.value().trim_end_matches('\n').to_string();
        match change.tag() {
            ChangeTag::Equal => {
                old_no += 1;
                new_no += 1;
                rows.push(DiffRow {
                    kind: DiffKind::Context,
                    old_no: Some(old_no),
                    new_no: Some(new_no),
                    text,
                });
            }
            ChangeTag::Delete => {
                old_no += 1;
                rows.push(DiffRow {
                    kind: DiffKind::Removed,
                    old_no: Some(old_no),
                    new_no: None,
                    text,
                });
            }
            ChangeTag::Insert => {
                new_no += 1;
                rows.push(DiffRow {
                    kind: DiffKind::Added,
                    old_no: None,
                    new_no: Some(new_no),
                    text,
                });
            }
        }
    }
    rows
}

/// 统计新增/删除行数（用于卡片标题）。
pub fn stats(rows: &[DiffRow]) -> (usize, usize) {
    let added = rows.iter().filter(|r| r.kind == DiffKind::Added).count();
    let removed = rows.iter().filter(|r| r.kind == DiffKind::Removed).count();
    (added, removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_added_and_removed_lines() {
        let rows = rows("a\nb\n", "a\nc\n");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].kind, DiffKind::Context);
        assert_eq!(rows[1].kind, DiffKind::Removed);
        assert_eq!(rows[1].text, "b");
        assert_eq!(rows[2].kind, DiffKind::Added);
        assert_eq!(rows[2].text, "c");
        assert_eq!(stats(&rows), (1, 1));
    }

    #[test]
    fn line_numbers_only_advance_on_matching_side() {
        let rows = rows("x\n", "x\ny\n");
        assert_eq!(rows[1].kind, DiffKind::Added);
        assert_eq!(rows[1].old_no, None);
        assert_eq!(rows[1].new_no, Some(2));
    }

    #[test]
    fn identical_input_has_no_changes() {
        let rows = rows("same\n", "same\n");
        assert_eq!(stats(&rows), (0, 0));
    }
}

/// 解析已经成型的 unified diff 文本（工具输出常见形态）。
///
/// 识别 `@@` hunk 头与 `+` / `-` / ` ` 前缀；`+++` / `---` 文件头跳过。
/// 不是 diff 的文本返回空 Vec，由调用方决定回退到普通展示。
pub fn parse_unified(text: &str) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    let mut old_no = 0usize;
    let mut new_no = 0usize;
    let mut saw_hunk = false;
    let mut saw_header = false;

    for line in text.lines() {
        if line.starts_with("--- ") || line.starts_with("+++ ") {
            saw_header = true;
            continue;
        }
        if line.starts_with("@@") {
            saw_hunk = true;
            continue;
        }
        let (kind, body) = match line.chars().next() {
            Some('+') => (DiffKind::Added, &line[1..]),
            Some('-') => (DiffKind::Removed, &line[1..]),
            Some(' ') => (DiffKind::Context, &line[1..]),
            _ => continue,
        };
        match kind {
            DiffKind::Added => {
                new_no += 1;
                rows.push(DiffRow {
                    kind,
                    old_no: None,
                    new_no: Some(new_no),
                    text: body.to_string(),
                });
            }
            DiffKind::Removed => {
                old_no += 1;
                rows.push(DiffRow {
                    kind,
                    old_no: Some(old_no),
                    new_no: None,
                    text: body.to_string(),
                });
            }
            DiffKind::Context => {
                old_no += 1;
                new_no += 1;
                rows.push(DiffRow {
                    kind,
                    old_no: Some(old_no),
                    new_no: Some(new_no),
                    text: body.to_string(),
                });
            }
        }
    }

    // 只有出现 `@@` hunk 或 `---/+++` 文件头才算 diff。
    // 否则 shell 输出里单独一行 `---` 会被误判成「删了一行」。
    if saw_hunk || saw_header {
        rows
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod unified_tests {
    use super::*;

    #[test]
    fn plain_output_with_dashes_is_not_a_diff() {
        let text = "/work/project\n---\n.\n..\nCargo.toml\n";
        assert!(parse_unified(text).is_empty(), "普通输出不能被当成 diff");
    }

    #[test]
    fn parses_hunk_with_all_line_kinds() {
        let text = "--- a/x.rs\n+++ b/x.rs\n@@ -1,3 +1,3 @@\n context\n-old\n+new\n";
        let rows = parse_unified(text);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].kind, DiffKind::Context);
        assert_eq!(rows[1].kind, DiffKind::Removed);
        assert_eq!(rows[1].text, "old");
        assert_eq!(rows[2].kind, DiffKind::Added);
        assert_eq!(stats(&rows), (1, 1));
    }

    #[test]
    fn plain_text_is_not_a_diff() {
        assert!(parse_unified("hello\nworld").is_empty());
    }
}
