//! 视图层的数据模型（与 wire 类型解耦）。

/// 会话的展示摘要。
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub workspace: String,
    pub status: SessionStatus,
    pub updated: String,
    /// 子会话的父会话 id；普通会话为 `None`。
    pub parent_id: Option<String>,
    /// `session/list` 里的 `origin == "subagent"`；普通侧栏不展示。
    pub is_subagent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Idle,
    Running,
    Waiting,
    Error,
}

impl SessionStatus {
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

/// 一个待回答的问题（来自 `user-questions/request` waterfall）。
#[derive(Debug, Clone, PartialEq)]
pub struct QuestionItemView {
    pub id: String,
    pub question: String,
    pub detail: Option<String>,
    pub options: Vec<String>,
    pub multi_select: bool,
}

/// 一张提问卡（一次 waterfall 可能带多个问题，当前按列表顺序渲染）。
#[derive(Debug, Clone, PartialEq)]
pub struct QuestionCard {
    pub event_id: String,
    pub agent_id: String,
    pub call_id: String,
    pub items: Vec<QuestionItemView>,
}

/// 一条待发消息（来自 `inbox` projection）。
#[derive(Debug, Clone, PartialEq)]
pub struct QueueItem {
    pub id: String,
    pub text: String,
}

/// 子 Agent 的生命周期模式（视图层，与 wire 解耦）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentMode {
    OneShot,
    Continuable,
    External,
    Unknown,
}

impl SubagentMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::OneShot => "one-shot",
            Self::Continuable => "continuable",
            Self::External => "external",
            Self::Unknown => "unknown",
        }
    }

    pub fn is_continuable(self) -> bool {
        matches!(self, Self::Continuable)
    }

    /// 只读：one-shot / unknown / external 都不接受续聊。
    pub fn is_read_only(self) -> bool {
        !self.is_continuable()
    }

    pub fn css_class(self) -> &'static str {
        match self {
            Self::OneShot => "subagent-mode subagent-mode--one-shot",
            Self::Continuable => "subagent-mode subagent-mode--continuable",
            Self::External => "subagent-mode subagent-mode--external",
            Self::Unknown => "subagent-mode subagent-mode--unknown",
        }
    }
}

/// 子 Agent 的活动状态（对应上游目录行的三种点）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentState {
    Running,
    Completed,
    Idle,
}

impl SubagentState {
    pub fn css_class(self) -> &'static str {
        match self {
            Self::Running => "dot dot--running",
            Self::Completed => "dot dot--ok",
            Self::Idle => "dot dot--idle",
        }
    }
}

/// 子 Agent 目录的一行（父会话的直接子级）。
#[derive(Debug, Clone, PartialEq)]
pub struct SubagentRow {
    pub id: String,
    pub parent_id: String,
    pub mode: SubagentMode,
    pub label: String,
    pub state: SubagentState,
    /// 已结算时长 + 当前打开 turn 的时长。
    pub elapsed_ms: u64,
    /// 四个互斥 token 桶之和。
    pub tokens: u64,
}

impl SubagentRow {
    pub fn is_running(&self) -> bool {
        self.state == SubagentState::Running
    }
}

/// 右栏侧开的子会话（轻量只读副本，续聊仍在主面板）。
#[derive(Debug, Clone, PartialEq)]
pub struct AsideChat {
    pub id: String,
    pub title: String,
    pub messages: Vec<MessageRow>,
    pub running: bool,
}
