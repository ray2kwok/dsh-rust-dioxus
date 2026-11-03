//! Subagent 目录、续聊与停止的 wire 契约。
//!
//! 上游出处：
//! - `packages/subagent/subagent/src/projection-types.ts`（`subagentCatalog` / `subagentTiming` / `subagent`）
//! - `packages/subagent/subagent/src/control-types.ts`（`SubagentPromptRequest` / `SubagentAddress`）
//! - `packages/subagent/subagent/src/index.ts`（`@Remote('prompt')` / `@Remote('interruptByParent')`）
//!
//! `args` 键名 = Host 侧形参名：续聊是 `{ request }`，停止是平铺的三个字段。
//! 这两条都必须按原样发送，写错会被 gateway 拒绝。

use serde::{Deserialize, Serialize};

use crate::session::{PromptContent, PromptMode, SessionId, SubagentMode};

/// `subagentCatalog` 的一行（父会话视角的直接子级）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCatalogEntry {
    pub id: SessionId,
    #[serde(default)]
    pub created_at: u64,
    pub mode: SubagentMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl SubagentCatalogEntry {
    /// 展示标题：优先 label，否则用短 id（由 UI 决定截断）。
    pub fn label_or_id(&self) -> &str {
        self.label.as_deref().unwrap_or(self.id.as_str())
    }
}

/// `subagentTiming`：已结算的 turn 时长 + 当前打开的 turn。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentTiming {
    /// 子会话自身 descriptor 之后、已完成 turn 的毫秒累计。
    #[serde(default)]
    pub settled_ms: u64,
    /// 当前尚未 `turn/end` 的 turn 边界。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<SubagentActiveTurn>,
    /// 最近一个已闭合 turn 是否正常完成；打开中或尚未闭合时缺省。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_turn_completed: Option<bool>,
}

impl SubagentTiming {
    pub fn is_running(&self) -> bool {
        self.active.is_some()
    }

    /// 显示用时长：已结算 + 当前 turn 的 `through - since`。
    pub fn elapsed_ms(&self) -> u64 {
        let live = self
            .active
            .as_ref()
            .map(|turn| turn.through.saturating_sub(turn.since))
            .unwrap_or(0);
        self.settled_ms.saturating_add(live)
    }
}

/// 当前打开的 turn 的同一切点边界。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentActiveTurn {
    pub since: u64,
    pub through: u64,
}

/// `subagent`：子会话自己的身份（来自 descriptor）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentIdentity {
    pub mode: SubagentMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub seq: u64,
}

/// `tokenUsage` 的四个互斥桶（`totalTokens` / `reasoningTokens` 不参与求和）。
///
/// 线上字段是 `uncachedInputTokens`（不是 `inputTokens`）；`inputTokens` 作为
/// 旧形状的别名保留。这条是 2026-11-03 对真实 Host 实测得到的。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    #[serde(rename = "uncachedInputTokens", alias = "inputTokens", default)]
    pub uncached_input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_write_tokens: u64,
}

impl TokenUsage {
    pub fn total(&self) -> u64 {
        self.uncached_input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.cache_read_tokens)
            .saturating_add(self.cache_write_tokens)
    }
}

/// `subagent/prompt` 请求。
///
/// 上游：`SubagentPromptRequest`。`mode` 必须是 `continuable`；
/// `delivery` 复用普通 prompt 的 `queue` / `steer`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentPromptRequest {
    /// 客户端铸造，会持久化到被接受的消息上。
    pub request_id: String,
    pub parent_session_id: SessionId,
    pub child_session_id: SessionId,
    pub mode: SubagentMode,
    pub delivery: PromptMode,
    pub content: Vec<PromptContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_time_zone: Option<String>,
}

/// `subagent/prompt` 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentPromptValue {
    pub message_id: String,
}

/// `subagent/interruptByParent` 的平铺 args。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentInterruptRequest {
    pub child_session_id: SessionId,
    pub parent_session_id: SessionId,
    pub mode: SubagentMode,
}

/// `subagent/interruptByParent` 响应。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentInterruptValue {
    pub accepted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn catalog_entry_parses_all_modes() {
        let raw = json!([
            { "id": "s1", "createdAt": 1, "mode": "one-shot", "label": "review" },
            { "id": "s2", "createdAt": 2, "mode": "continuable", "label": "worker" },
            { "id": "s3", "createdAt": 3, "mode": "external" },
            { "id": "s4", "createdAt": 4, "mode": "future-mode" }
        ]);
        let rows: Vec<SubagentCatalogEntry> = serde_json::from_value(raw).unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].mode, SubagentMode::OneShot);
        assert_eq!(rows[1].mode, SubagentMode::Continuable);
        assert_eq!(rows[2].mode, SubagentMode::External);
        assert_eq!(
            rows[3].mode,
            SubagentMode::Unknown,
            "未知模式不能拖垮整棵目录"
        );
        assert_eq!(rows[1].label_or_id(), "worker");
        assert_eq!(rows[2].label_or_id(), "s3");
    }

    #[test]
    fn prompt_request_matches_upstream_args() {
        let request = SubagentPromptRequest {
            request_id: "req-1".into(),
            parent_session_id: SessionId::new("parent"),
            child_session_id: SessionId::new("child"),
            mode: SubagentMode::Continuable,
            delivery: PromptMode::Steer,
            content: vec![PromptContent::text("continue")],
            client_time_zone: Some("Asia/Shanghai".into()),
        };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "requestId": "req-1",
                "parentSessionId": "parent",
                "childSessionId": "child",
                "mode": "continuable",
                "delivery": "steer",
                "content": [{ "type": "text", "text": "continue" }],
                "clientTimeZone": "Asia/Shanghai"
            })
        );
    }

    #[test]
    fn interrupt_args_are_flat() {
        let request = SubagentInterruptRequest {
            child_session_id: SessionId::new("child"),
            parent_session_id: SessionId::new("parent"),
            mode: SubagentMode::Continuable,
        };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "childSessionId": "child",
                "parentSessionId": "parent",
                "mode": "continuable"
            })
        );
    }

    #[test]
    fn timing_sums_settled_and_open_turn() {
        let timing: SubagentTiming = serde_json::from_value(json!({
            "settledMs": 1500,
            "active": { "since": 10_000, "through": 12_000 },
            "lastTurnCompleted": false
        }))
        .unwrap();
        assert!(timing.is_running());
        assert_eq!(timing.elapsed_ms(), 3500);
        assert_eq!(timing.last_turn_completed, Some(false));
    }

    #[test]
    fn token_usage_sums_four_buckets() {
        // 真实 Host 的字段是 uncachedInputTokens
        let usage: TokenUsage = serde_json::from_value(json!({
            "uncachedInputTokens": 10,
            "outputTokens": 20,
            "cacheReadTokens": 3,
            "cacheWriteTokens": 4,
            "totalTokens": 999
        }))
        .unwrap();
        assert_eq!(usage.total(), 37);

        // 旧形状（inputTokens）仍可解析
        let legacy: TokenUsage = serde_json::from_value(json!({ "inputTokens": 5 })).unwrap();
        assert_eq!(legacy.total(), 5);
    }
}
