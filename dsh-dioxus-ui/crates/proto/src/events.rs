//! `$events` 逻辑流：Host → 客户端的通知与 waterfall。
//!
//! 上游出处：
//! - `packages/api/gateway/src/stream-protocol.ts`
//!   （`REMOTE_EVENT_STREAM_ENDPOINT` / `RemoteEventDownlinkFrame` / `RemoteEventResult`）
//!
//! 这条流有两个特殊地位：
//! 1. 它的 `ready` 帧是**连接代次（generation）的唯一来源**，重连后旧 `clientId` 立即作废；
//! 2. 审批（`approval/request`）等 waterfall 的应答不在这条流上回，
//!    而是走一元 `$events/result`。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 事件流端点字面量（不是 `/api/<ns>/<method>` 形式）。
pub const REMOTE_EVENT_STREAM_ENDPOINT: &str = "$events";
/// waterfall 应答的一元端点。
pub const REMOTE_EVENT_RESULT_ENDPOINT: &str = "$events/result";
/// 打开事件流时 `payload` 必须**恰好**是 `{ "args": {} }`。
pub const REMOTE_EVENT_STREAM_PAYLOAD: &str = r#"{ "args": {} }"#;

/// `ready` 帧附带的 Host 事实。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostInfo {
    /// 本机 home 目录，仅用于把绝对路径缩略显示。
    pub home: String,
}

/// `$events` 下行帧。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RemoteEventFrame {
    /// 必须是第一帧；`client_id` 用于后续 waterfall 应答。
    Ready {
        #[serde(rename = "clientId")]
        client_id: String,
        host: HostInfo,
    },
    /// 普通通知，不重放、不需要应答。
    Emit { event: String, args: Vec<Value> },
    /// 需要客户端决定的 waterfall（如 `approval/request`）。
    ///
    /// 注意 `request` 里**不含** `agent` 与 `signal`（由客户端上下文补），
    /// 且 `agentId` 是并行字段。
    Waterfall {
        event: String,
        #[serde(rename = "eventId")]
        event_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        request: Value,
    },
    /// 之前投递的 waterfall 已失效（例如 turn 被中断）。
    Cancel {
        #[serde(rename = "eventId")]
        event_id: String,
    },
}

/// 回答 waterfall 的一元请求体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteEventResult {
    #[serde(rename = "clientId")]
    pub client_id: String,
    #[serde(rename = "eventId")]
    pub event_id: String,
    pub outcome: EventOutcome,
}

impl RemoteEventResult {
    pub fn new(
        client_id: impl Into<String>,
        event_id: impl Into<String>,
        outcome: EventOutcome,
    ) -> Self {
        Self {
            client_id: client_id.into(),
            event_id: event_id.into(),
            outcome,
        }
    }
}

/// waterfall 的处置结果。`kind` 的取值域由上游固定。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum EventOutcome {
    /// 交给下一个监听者（客户端不作为最终裁决）。
    Next,
    /// 给出结果值（审批时为 [`ApprovalOutcome`]）。
    Result {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<Value>,
    },
    /// 监听者抛错。
    Rejected { error: EventRejection },
}

impl EventOutcome {
    /// 以任意可序列化值作为最终结果。
    pub fn result(value: impl Serialize) -> Self {
        Self::Result {
            value: serde_json::to_value(value).ok(),
        }
    }

    /// 审批场景：`allowed-once` / `rejected` / `cancelled` / `unavailable`。
    pub fn approval(outcome: ApprovalOutcome) -> Self {
        Self::result(outcome)
    }
}

/// 审批结论。
///
/// 上游出处：`packages/interaction/user-approval/src/types.ts`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalOutcome {
    #[serde(rename = "allowed-once")]
    AllowedOnce,
    #[serde(rename = "rejected")]
    Rejected,
    #[serde(rename = "cancelled")]
    Cancelled,
    #[serde(rename = "unavailable")]
    Unavailable,
}

/// waterfall 处理失败的形状。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventRejection {
    pub name: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ready_frame() {
        let frame: RemoteEventFrame = serde_json::from_value(json!({
            "type": "ready",
            "clientId": "client-1",
            "host": { "home": "/home/me" }
        }))
        .unwrap();
        match frame {
            RemoteEventFrame::Ready { client_id, host } => {
                assert_eq!(client_id, "client-1");
                assert_eq!(host.home, "/home/me");
            }
            other => panic!("期望 ready，实际 {other:?}"),
        }
    }

    #[test]
    fn emit_frame_keeps_arbitrary_args() {
        let frame: RemoteEventFrame = serde_json::from_value(json!({
            "type": "emit",
            "event": "api-session/status",
            "args": ["s1", true]
        }))
        .unwrap();
        match frame {
            RemoteEventFrame::Emit { event, args } => {
                assert_eq!(event, "api-session/status");
                assert_eq!(args, vec![json!("s1"), json!(true)]);
            }
            other => panic!("期望 emit，实际 {other:?}"),
        }
    }

    #[test]
    fn waterfall_frame_carries_agent_id() {
        let frame: RemoteEventFrame = serde_json::from_value(json!({
            "type": "waterfall",
            "event": "approval/request",
            "eventId": "ev-1",
            "agentId": "s1",
            "request": { "tool": "bash", "input": { "cmd": "ls" } }
        }))
        .unwrap();
        match frame {
            RemoteEventFrame::Waterfall {
                event_id,
                agent_id,
                request,
                ..
            } => {
                assert_eq!(event_id, "ev-1");
                assert_eq!(agent_id, "s1");
                assert_eq!(request["tool"], json!("bash"));
            }
            other => panic!("期望 waterfall，实际 {other:?}"),
        }
    }

    #[test]
    fn approval_result_serializes_to_wire_shape() {
        let result = RemoteEventResult::new(
            "client-1",
            "ev-1",
            EventOutcome::approval(ApprovalOutcome::AllowedOnce),
        );
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            json!({
                "clientId": "client-1",
                "eventId": "ev-1",
                "outcome": { "kind": "result", "value": "allowed-once" }
            })
        );
    }

    #[test]
    fn cancel_outcome_omits_value() {
        let outcome = EventOutcome::Result { value: None };
        assert_eq!(
            serde_json::to_value(&outcome).unwrap(),
            json!({ "kind": "result" })
        );
    }
}
