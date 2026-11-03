//! M0 阶段的假数据。
//!
//! 存在的意义：在没有 Host 的情况下验证布局、滚动、输入回路。
//! 接入 `dsh-state` 后整个模块删除。

use dsh_ui::{MessageRow, Role, SessionStatus, SessionSummary};

pub fn sessions() -> Vec<SessionSummary> {
    vec![
        SessionSummary {
            id: "s-1".into(),
            title: "把 webui 翻译成 Dioxus".into(),
            workspace: "dsh-dioxus".into(),
            status: SessionStatus::Running,
            updated: "刚刚".into(),
        },
        SessionSummary {
            id: "s-2".into(),
            title: "协议层：follow 帧语义".into(),
            workspace: "dsh-dioxus".into(),
            status: SessionStatus::Idle,
            updated: "12 分钟前".into(),
        },
        SessionSummary {
            id: "s-3".into(),
            title: "RGNN 生长实验复盘".into(),
            workspace: "rg-note".into(),
            status: SessionStatus::Waiting,
            updated: "昨天".into(),
        },
    ]
}

pub fn messages() -> Vec<MessageRow> {
    vec![
        MessageRow::new("m-1", Role::User, "先看看三栏骨架跑不跑得起来。"),
        MessageRow::new(
            "m-2",
            Role::Assistant,
            "骨架已就绪：左栏会话、中间消息流、右栏可折叠。下一步接 HostSupervisor。",
        ),
        MessageRow::new("m-3", Role::System, "生成 3 个文件，耗时 1.2s。"),
        MessageRow::new("m-4", Role::Tool, "cargo test -p dsh-state").tool("bash"),
        MessageRow::new(
            "m-5",
            Role::Assistant,
            "6 个测试通过，其中 compaction 的 replace 语义有专门断言。",
        ),
    ]
}
