//! 视图层的数据模型（与 wire 类型解耦）。

/// 会话的展示摘要。
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub workspace: String,
    pub status: SessionStatus,
    pub updated: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Idle,
    Running,
    Waiting,
    Error,
}

impl SessionStatus {
    pub fn label(self) -> &'static str {
        match self {
            SessionStatus::Idle => "空闲",
            SessionStatus::Running => "运行中",
            SessionStatus::Waiting => "待确认",
            SessionStatus::Error => "错误",
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            SessionStatus::Idle => "dot dot--idle",
            SessionStatus::Running => "dot dot--running",
            SessionStatus::Waiting => "dot dot--waiting",
            SessionStatus::Error => "dot dot--error",
        }
    }
}

/// 消息流的角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Role::User => "你",
            Role::Assistant => "助手",
            Role::System => "系统",
            Role::Tool => "工具",
        }
    }
}

/// 一条可渲染的消息。
#[derive(Debug, Clone, PartialEq)]
pub struct MessageRow {
    pub id: String,
    pub role: Role,
    pub text: String,
    pub streaming: bool,
    pub tool_name: Option<String>,
}

impl MessageRow {
    pub fn new(id: impl Into<String>, role: Role, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            role,
            text: text.into(),
            streaming: false,
            tool_name: None,
        }
    }

    pub fn streaming(mut self, streaming: bool) -> Self {
        self.streaming = streaming;
        self
    }

    pub fn tool(mut self, name: impl Into<String>) -> Self {
        self.tool_name = Some(name.into());
        self
    }
}

/// 连接状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
}

impl ConnectionState {
    pub fn label(self) -> &'static str {
        match self {
            ConnectionState::Disconnected => "未连接",
            ConnectionState::Connecting => "连接中",
            ConnectionState::Connected => "已连接",
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            ConnectionState::Disconnected => "badge badge--idle",
            ConnectionState::Connecting => "badge badge--waiting",
            ConnectionState::Connected => "badge badge--ok",
        }
    }
}

/// 一条待处理的审批请求。
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalItem {
    pub event_id: String,
    pub agent_id: String,
    pub title: String,
    pub detail: String,
}

/// 发送模式（协议里的 `queue` / `steer`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendMode {
    Queue,
    Steer,
}

impl SendMode {
    pub fn label(self) -> &'static str {
        match self {
            SendMode::Queue => "排队",
            SendMode::Steer => "插话",
        }
    }

    pub fn css_class(self, active: bool) -> &'static str {
        match (self, active) {
            (SendMode::Queue, true) => "seg seg--active",
            (SendMode::Queue, false) => "seg",
            (SendMode::Steer, true) => "seg seg--active seg--steer",
            (SendMode::Steer, false) => "seg seg--steer",
        }
    }
}

/// 主题偏好。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
    System,
}

impl Theme {
    pub fn label(self) -> &'static str {
        match self {
            Theme::Light => "浅色",
            Theme::Dark => "深色",
            Theme::System => "跟随系统",
        }
    }

    /// 写入 `<html data-theme>` 的值；`system` 交给 CSS 媒体查询。
    pub fn attribute(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::System => "system",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::System,
            Theme::System => Theme::Light,
        }
    }
}

/// Trajectory 视图的一行（原始事件的轻量展示）。
#[derive(Debug, Clone, PartialEq)]
pub struct TrajectoryRow {
    pub seq: u64,
    pub kind: String,
    pub detail: String,
}

/// 右栏文件树的一项。
#[derive(Debug, Clone, PartialEq)]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
}
