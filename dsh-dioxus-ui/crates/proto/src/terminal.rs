//! 用户终端（`terminal/*`）契约。
//!
//! 上游出处：`packages/api/terminal-controller/src/types.ts`。
//!
//! 与 Agent 自己的终端工具不同：这是**用户交互式 shell**，由 Session 拥有。
//! 关键约束：
//! - `id` 由客户端生成（`^[\w-]{1,128}$`），`create` 对同一 id 幂等；
//! - `attach` 是**双向**流：下行 `TerminalFrame`，上行 `TerminalInput`；
//! - 每条 attachment 的 `sequence` 从 1 连续递增，Host 不重放；
//! - 首帧一定是完整屏幕快照（ANSI 文本），之后才是增量输出。

use serde::{Deserialize, Serialize};

/// 已在本机验证可执行的 shell。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalShell {
    pub path: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub name: String,
}

/// Session 当前目录与终端限制。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalEnvironment {
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub max_input_bytes: u64,
    #[serde(default)]
    pub max_buffered_bytes: u64,
    #[serde(default)]
    pub max_cols: u32,
    #[serde(default)]
    pub max_rows: u32,
    #[serde(default)]
    pub scrollback: u32,
}

/// Host 持有的终端状态；进程退出不会自动换新 shell。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebTerminalInfo {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub shell: Option<TerminalShell>,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub cols: u32,
    #[serde(default)]
    pub rows: u32,
    /// `running` / `exited` / `failed`；保留字符串以免上游新增状态时解析失败。
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub exit_code: Option<i64>,
    #[serde(default)]
    pub error: Option<String>,
}

impl WebTerminalInfo {
    pub fn is_running(&self) -> bool {
        self.state == "running"
    }

    pub fn is_writable(&self) -> bool {
        self.is_running()
    }
}

/// `create` 的请求：id 由客户端 mint，重复调用返回同一终端。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCreateRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_path: Option<String>,
    pub id: String,
    pub cols: u32,
    pub rows: u32,
}

/// 客户端 → Host 的上行帧。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TerminalInput {
    Write { sequence: u64, data: String },
    Resize { sequence: u64, cols: u32, rows: u32 },
}

impl TerminalInput {
    pub fn write(sequence: u64, data: impl Into<String>) -> Self {
        Self::Write {
            sequence,
            data: data.into(),
        }
    }

    pub fn resize(sequence: u64, cols: u32, rows: u32) -> Self {
        Self::Resize {
            sequence,
            cols,
            rows,
        }
    }
}

/// Host → 客户端的下行帧。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TerminalFrame {
    /// 每条 attachment 的第一帧：完整、有界的屏幕（含 ANSI）。
    Snapshot {
        #[serde(default)]
        sequence: u64,
        #[serde(default)]
        screen: String,
        info: WebTerminalInfo,
        #[serde(default)]
        writable: bool,
    },
    /// 有序增量输出。
    Output {
        #[serde(default)]
        sequence: u64,
        #[serde(default)]
        data: String,
    },
    /// 元信息变化（重命名 / 进程退出）。
    State {
        info: WebTerminalInfo,
        #[serde(default)]
        writable: bool,
    },
    /// 上行确认；`refusal` 存在表示该条输入被拒（只读 / 进程已退出）。
    Ack {
        #[serde(default)]
        sequence: u64,
        #[serde(default)]
        refusal: Option<String>,
    },
}

/// `retain` 流的第一帧：仅确认窗口持有，不接管屏幕与输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TerminalRetentionFrame {
    Retained,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn info_tolerates_missing_optional_fields() {
        let info: WebTerminalInfo = serde_json::from_value(json!({
            "id": "t-1",
            "title": "bash",
            "cwd": "/tmp",
            "cols": 80,
            "rows": 24,
            "state": "running",
            "exitCode": null
        }))
        .unwrap();
        assert!(info.is_running());
        assert!(info.shell.is_none());
    }

    #[test]
    fn input_frames_use_camel_case_and_type_tag() {
        assert_eq!(
            serde_json::to_value(TerminalInput::write(1, "ls\n")).unwrap(),
            json!({ "type": "write", "sequence": 1, "data": "ls\n" })
        );
        assert_eq!(
            serde_json::to_value(TerminalInput::resize(2, 100, 30)).unwrap(),
            json!({ "type": "resize", "sequence": 2, "cols": 100, "rows": 30 })
        );
    }

    #[test]
    fn frame_variants_parse() {
        let snapshot: TerminalFrame = serde_json::from_value(json!({
            "type": "snapshot",
            "sequence": 0,
            "screen": "\u{1b}[31m$\u{1b}[0m ",
            "info": { "id": "t-1", "title": "bash", "cwd": "/tmp", "cols": 80, "rows": 24,
                      "state": "running", "exitCode": null },
            "writable": true,
            "inputWindow": { "limit": 64 }
        }))
        .unwrap();
        match snapshot {
            TerminalFrame::Snapshot {
                screen, writable, ..
            } => {
                assert!(screen.contains("$"));
                assert!(writable);
            }
            other => panic!("期望 snapshot，实际 {other:?}"),
        }

        let ack: TerminalFrame =
            serde_json::from_value(json!({ "type": "ack", "sequence": 3 })).unwrap();
        assert!(matches!(ack, TerminalFrame::Ack { refusal: None, .. }));
    }

    #[test]
    fn create_request_omits_absent_shell() {
        let request = TerminalCreateRequest {
            shell_path: None,
            id: "t-1".into(),
            cols: 80,
            rows: 24,
        };
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            json!({ "id": "t-1", "cols": 80, "rows": 24 })
        );
    }
}
