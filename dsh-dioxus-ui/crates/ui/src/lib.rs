//! dsh-dioxus-ui 的组件层。
//!
//! 结构对照上游：
//!
//! | 上游 | 本项目 |
//! | --- | --- |
//! | `ui-layout` / `AppFrame` | [`layout::AppShell`] |
//! | `ui-sidebar` | [`sidebar::SessionList`] |
//! | `ui-conversation` + `ui-chat` | [`conversation::ConversationView`] |
//!
//! 纪律（与上游 `packages/client/AGENTS.md` 对齐）：
//! - 组件只读 props / signal，不持有连接、不发 RPC；
//! - 副作用（滚动、焦点）只出现在 `use_effect`；
//! - 数据模型用 [`model`] 里的视图类型，不直接依赖 wire 类型。

pub mod composer;
pub mod conversation;
pub mod editor;
pub mod file_tree;
pub mod i18n;
pub mod icons;
pub mod layout;
pub mod markdown;
pub mod math;
pub mod model;
pub mod settings;
pub mod sidebar;
pub mod subagent;
pub mod terminal;
pub mod tool_card;

pub use composer::{CommandItemView, Composer, FileRefItemView};
pub use editor::EditorPane;
pub use file_tree::FileTree;
pub use i18n::{Key, Locale, T};
pub use icons::Icon;
pub use layout::AppShell;
pub use markdown::MarkdownView;
pub use math::{MathChunk, MathText, MathView, split_math};
pub use model::{
    ApprovalItem, AsideChat, ComposerAttachment, ConnectionState, FileEntry, JobRow, MessageRow,
    PlanState, PreviewDoc, QuestionCard, QuestionItemView, QueueItem, Role, SearchHit, SendMode,
    SendRequest, SessionStatus, SessionSummary, SubagentMode, SubagentRow, SubagentState,
    TerminalView, Theme, TrajectoryRow, WorkspaceTab,
};
pub use settings::{
    AgentPresetItemView, ModelGroupView, ModelItemView, PermissionItemView, ProviderRow,
    SettingsChange, SettingsPanel, SettingsSnapshot,
};
pub use subagent::{SubagentAside, SubagentCatalog};
pub use terminal::TerminalPanel;
pub use tool_card::ToolCard;
