//! 视图层的数据模型。
//!
//! 刻意与 `dsh-proto` 的 wire 类型分开：协议类型描述「Host 说什么」，
//! 这里的类型描述「界面需要什么」。中间由 `dsh-state` 负责转换，
//! 因此协议演进（新增事件类型）不会直接冲击组件。

/// 会话的展示摘要。
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    /// 所属工作区名（用于分组）。
    pub workspace: String,
    pub status: SessionStatus,
    /// 已格式化的相对时间，例如「3 分钟前」。
    pub updated: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    /// 空闲，可发送。
    Idle,
    /// 正在跑 turn。
    Running,
    /// 等待用户审批/回答。
    Waiting,
    /// 出错。
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

    pub fn css_class(self) -> &'static str {
        match self {
            Role::User => "msg msg--user",
            Role::Assistant => "msg msg--assistant",
            Role::System => "msg msg--system",
            Role::Tool => "msg msg--tool",
        }
    }
}

/// 一条可渲染的消息。
#[derive(Debug, Clone, PartialEq)]
pub struct MessageRow {
    pub id: String,
    pub role: Role,
    pub text: String,
    /// 是否仍在流式输出（用于显示光标/动画）。
    pub streaming: bool,
    /// 工具卡标题（`Role::Tool` 时使用）。
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

/// 连接状态（显示在顶栏）。
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
