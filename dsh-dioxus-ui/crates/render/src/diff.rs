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
