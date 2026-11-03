//! 轻量 i18n。
//!
//! 上游用类型化字典（`ctx.locale.register(ns, {zh, en, …})`）+ `t` seat 实现
//! 8 语言。这里保持同样的思路但收敛到最小实现：
//!
//! - **键是枚举**（`Key`），拼错编译期报错；
//! - 字典是 `match`，零运行时分配、零依赖；
//! - 语言通过 Dioxus context 下发（`Signal<Locale>`），切换时自动重渲。
//!
//! 新增文案的流程：加一个 `Key` → 在两个 `match` 分支补文案 → 组件里 `t.get(Key::X)`。

use dioxus::prelude::*;

use crate::model::{ConnectionState, Role, SendMode};

/// 支持的语言。上游内置 8 种，这里先做中英两套。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    Zh,
    En,
}

impl Locale {
    pub fn code(self) -> &'static str {
        match self {
            Locale::Zh => "zh",
            Locale::En => "en",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Locale::Zh => "中文",
            Locale::En => "English",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Locale::Zh => Locale::En,
            Locale::En => Locale::Zh,
        }
    }
}

/// 界面文案键。枚举而非字符串，写错就编译不过。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    NewSession,
    Sessions,
    Chat,
    Trajectory,
    NoMessages,
    ComposerPlaceholder,
    ComposerDisabled,
    Queue,
    Steer,
    Send,
    Stop,
    Settings,
    Theme,
    Language,
    Generation,
    SessionCount,
    Reconnect,
    Close,
    AllowOnce,
    Reject,
    NeedsConfirm,
    NeedsAnswer,
    OtherPlaceholder,
    SubmitAnswer,
    Connected,
    Connecting,
    Disconnected,
    StatusIdle,
    StatusRunning,
    StatusWaiting,
    StatusError,
    RoleUser,
    RoleAssistant,
    RoleSystem,
    RoleTool,
    SelectSession,
    SelectSessionHint,
    Files,
    Preview,
    FilesEmpty,
    Archive,
    RenameHint,
    Host,
    LoadEarlier,
}

/// 取文案。
pub fn text(locale: Locale, key: Key) -> &'static str {
    match locale {
        Locale::Zh => zh(key),
        Locale::En => en(key),
    }
}

fn zh(key: Key) -> &'static str {
    match key {
        Key::NewSession => "新建会话",
        Key::Sessions => "会话",
        Key::Chat => "对话",
        Key::Trajectory => "轨迹",
        Key::NoMessages => "还没有消息。发一条试试。",
        Key::ComposerPlaceholder => "输入消息，Enter 发送 / Shift+Enter 换行",
        Key::ComposerDisabled => "未连接，暂不可发送",
        Key::Queue => "排队",
        Key::Steer => "插话",
        Key::Send => "发送",
        Key::Stop => "停止",
        Key::Settings => "设置",
        Key::Theme => "主题",
        Key::Language => "语言",
        Key::Generation => "连接代次",
        Key::SessionCount => "会话数",
        Key::Reconnect => "重新连接",
        Key::Close => "关闭",
        Key::AllowOnce => "允许一次",
        Key::Reject => "拒绝",
        Key::NeedsConfirm => "需要确认",
        Key::NeedsAnswer => "需要回答",
        Key::OtherPlaceholder => "其他（可选）",
        Key::SubmitAnswer => "提交回答",
        Key::Connected => "已连接",
        Key::Connecting => "连接中",
        Key::Disconnected => "未连接",
        Key::StatusIdle => "空闲",
        Key::StatusRunning => "运行中",
        Key::StatusWaiting => "待确认",
        Key::StatusError => "错误",
        Key::RoleUser => "你",
        Key::RoleAssistant => "助手",
        Key::RoleSystem => "系统",
        Key::RoleTool => "工具",
        Key::SelectSession => "选择或新建一个会话",
        Key::SelectSessionHint => "左侧列表来自 Host 的 session/list。",
        Key::Files => "文件",
        Key::Preview => "预览",
        Key::FilesEmpty => "目录为空或尚未加载。",
        Key::Archive => "归档会话",
        Key::RenameHint => "点击重命名",
        Key::Host => "Host",
        Key::LoadEarlier => "加载更早的消息",
    }
}

fn en(key: Key) -> &'static str {
    match key {
        Key::NewSession => "New session",
        Key::Sessions => "Sessions",
        Key::Chat => "Chat",
        Key::Trajectory => "Trajectory",
        Key::NoMessages => "No messages yet. Say something.",
        Key::ComposerPlaceholder => "Message… Enter to send, Shift+Enter for newline",
        Key::ComposerDisabled => "Not connected",
        Key::Queue => "Queue",
        Key::Steer => "Steer",
        Key::Send => "Send",
        Key::Stop => "Stop",
        Key::Settings => "Settings",
        Key::Theme => "Theme",
        Key::Language => "Language",
        Key::Generation => "Generation",
        Key::SessionCount => "Sessions",
        Key::Reconnect => "Reconnect",
        Key::Close => "Close",
        Key::AllowOnce => "Allow once",
        Key::Reject => "Reject",
        Key::NeedsConfirm => "Needs confirmation",
        Key::NeedsAnswer => "Needs an answer",
        Key::OtherPlaceholder => "Other (optional)",
        Key::SubmitAnswer => "Submit",
        Key::Connected => "Connected",
        Key::Connecting => "Connecting",
        Key::Disconnected => "Disconnected",
        Key::StatusIdle => "Idle",
        Key::StatusRunning => "Running",
        Key::StatusWaiting => "Waiting",
        Key::StatusError => "Error",
        Key::RoleUser => "You",
        Key::RoleAssistant => "Assistant",
        Key::RoleSystem => "System",
        Key::RoleTool => "Tool",
        Key::SelectSession => "Select or create a session",
        Key::SelectSessionHint => "The list on the left comes from session/list.",
        Key::Files => "Files",
        Key::Preview => "Preview",
        Key::FilesEmpty => "Empty or not loaded yet.",
        Key::Archive => "Archive session",
        Key::RenameHint => "Click to rename",
        Key::Host => "Host",
        Key::LoadEarlier => "Load earlier messages",
    }
}

/// 组件内取文案的句柄（`Copy`，可随意捕获）。
#[derive(Clone, Copy)]
pub struct T(Locale);

impl T {
    pub fn get(self, key: Key) -> &'static str {
        text(self.0, key)
    }

    pub fn locale(self) -> Locale {
        self.0
    }
}

/// 从 context 取当前语言句柄。
///
/// App 根部必须 `use_context_provider(|| Signal::new(Locale::Zh))`；
/// 未提供时退回中文，而不是 panic（组件可独立渲染）。
pub fn use_t() -> T {
    match try_use_context::<Signal<Locale>>() {
        Some(signal) => T(*signal.read()),
        None => T(Locale::Zh),
    }
}

/// 连接状态 → 文案键。
pub fn connection_key(state: ConnectionState) -> Key {
    match state {
        ConnectionState::Connected => Key::Connected,
        ConnectionState::Connecting => Key::Connecting,
        ConnectionState::Disconnected => Key::Disconnected,
    }
}

/// 角色 → 文案键。
pub fn role_key(role: Role) -> Key {
    match role {
        Role::User => Key::RoleUser,
        Role::Assistant => Key::RoleAssistant,
        Role::System => Key::RoleSystem,
        Role::Tool => Key::RoleTool,
    }
}

/// 发送模式 → 文案键。
pub fn send_mode_key(mode: SendMode) -> Key {
    match mode {
        SendMode::Queue => Key::Queue,
        SendMode::Steer => Key::Steer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_has_both_locales() {
        // 用几个代表性的键确保两套字典都覆盖
        for key in [Key::Send, Key::Settings, Key::NeedsAnswer, Key::FilesEmpty] {
            assert!(!text(Locale::Zh, key).is_empty());
            assert!(!text(Locale::En, key).is_empty());
        }
    }

    #[test]
    fn locale_cycles() {
        assert_eq!(Locale::Zh.next(), Locale::En);
        assert_eq!(Locale::En.next(), Locale::Zh);
    }
}
