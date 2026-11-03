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

pub mod conversation;
pub mod i18n;
pub mod icons;
pub mod layout;
pub mod markdown;
pub mod model;
pub mod sidebar;
pub mod tool_card;

pub use i18n::{Key, Locale, T};
pub use icons::Icon;
pub use layout::AppShell;
pub use markdown::MarkdownView;
pub use model::{
    ApprovalItem, ConnectionState, FileEntry, MessageRow, QuestionCard, QuestionItemView, Role,
    SendMode, SessionStatus, SessionSummary, Theme, TrajectoryRow,
};
pub use tool_card::ToolCard;
