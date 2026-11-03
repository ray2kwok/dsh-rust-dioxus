//! 一元端点常量与 URL 拼接。
//!
//! 上游出处：`packages/api/gateway/src/index.ts`（`/api/<namespace>/<method>`）
//!
//! **`args` 键名不是方法名**，而是 Host 侧被装饰方法的形参名。这里用常量把
//! 已验证的事实固化下来，未验证的标注 `TODO(verify)`。
//!
//! 已确认（有测试/源码依据）：
//! - `session/page|follow|create|prompt|cancel|projections` → `{"request": …}`
//! - `$events/result` → `{"clientId", "eventId", "outcome"}`（无 `request` 外壳）
//! - `Agent` 形参线上叫 `agentId`；`Session` 形参叫 `sessionId`
//!
//! 实测确认（2026-11-03，对源码形态的 Host 跑 `examples/probe.rs`）：
//! - `session/list` 的 `args` 必须是 `{ "_request": {} }`；传 `{ "request": {} }` 会被
//!   `gateway/arguments-invalid` 拒绝（"missing \"_request\"; unexpected \"request\""）。
//!
//! TODO(verify)：`workspaceFiles/*` 的 `workspaceFileScopeId`、`commands/execute` 的键集。

/// API 前缀。
pub const API_PREFIX: &str = "/api";

/// 拼接一元端点 URL：`/api/session/prompt`。
pub fn api_path(endpoint: &str) -> String {
    format!("{API_PREFIX}/{endpoint}")
}

/// 会话命名空间。
pub mod session {
    pub const LIST: &str = "session/list";
    pub const SEARCH: &str = "session/search";
    pub const CREATE: &str = "session/create";
    pub const FORK: &str = "session/fork";
    pub const RENAME: &str = "session/rename";
    pub const PROMPT: &str = "session/prompt";
    pub const CANCEL: &str = "session/cancel";
    pub const PAGE: &str = "session/page";
    pub const FOLLOW: &str = "session/follow";
    pub const CONTROL: &str = "session/control";
    pub const PROJECTIONS: &str = "session/projections";
    pub const ATTACHMENT: &str = "session/attachment";
    pub const UPDATE_QUEUE: &str = "session/updateQueue";
    pub const SELECT_MODEL: &str = "session/selectModel";
    pub const MODEL_CATALOG: &str = "session/modelCatalog";
}

/// 工作区命名空间。
pub mod workspace {
    pub const FOLLOW: &str = "workspace/follow";
    pub const CREATE: &str = "workspace/create";
    pub const RENAME: &str = "workspace/rename";
    pub const DELETE: &str = "workspace/delete";
    pub const ARCHIVE_SESSION: &str = "workspace/archiveSession";
    pub const UNARCHIVE_SESSION: &str = "workspace/unarchiveSession";
    pub const PIN_SESSION: &str = "workspace/pinSession";
    pub const UNPIN_SESSION: &str = "workspace/unpinSession";
}

/// 设置与凭据。
pub mod settings {
    pub const DESCRIBE: &str = "settings/describe";
    pub const UPDATE: &str = "settings/update";
    pub const REPLACE: &str = "settings/replace";
    pub const MUTATE: &str = "settings/mutate";
}

/// 交互类。
pub mod interaction {
    pub const COMMANDS_LIST: &str = "commands/list";
    pub const COMMANDS_EXECUTE: &str = "commands/execute";
    pub const USER_QUESTIONS_ANSWER: &str = "userQuestions/answer";
    pub const PERMISSION_PRESETS: &str = "permissionPresets/catalog";
    pub const FILE_REFERENCES_LIST: &str = "fileReferences/list";
}

/// 文件系统。
pub mod files {
    pub const LIST: &str = "workspaceFiles/list";
    pub const READ: &str = "workspaceFiles/read";
    pub const READ_BYTES: &str = "workspaceFiles/readBytes";
    pub const STAT: &str = "workspaceFiles/stat";
    pub const CHANGES: &str = "workspaceFiles/changes";
}

/// 子 Agent 控制面（namespace 是复数 `subagents`，实测 404 才确认）。
pub mod subagent {
    pub const PROMPT: &str = "subagents/prompt";
    pub const INTERRUPT_BY_PARENT: &str = "subagents/interruptByParent";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_path_joins_namespace() {
        assert_eq!(api_path(session::PROMPT), "/api/session/prompt");
        assert_eq!(api_path("$events/result"), "/api/$events/result");
    }

    #[test]
    fn endpoint_constants_match_upstream_names() {
        // 这些字符串直接决定请求成败，改动必须同步上游类型定义
        assert_eq!(session::FOLLOW, "session/follow");
        assert_eq!(session::PAGE, "session/page");
        assert_eq!(interaction::USER_QUESTIONS_ANSWER, "userQuestions/answer");
        assert_eq!(files::LIST, "workspaceFiles/list");
    }
}
