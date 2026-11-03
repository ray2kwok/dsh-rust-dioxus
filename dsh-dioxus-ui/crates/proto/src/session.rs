//! 会话（Session）相关契约。
//!
//! 上游出处：
//! - `packages/api/session-controller/src/types.ts`（请求/响应/帧）
//! - `packages/core/session/src/types.ts`（durable 事件 `SessionEventMap`）
//!
//! 两条重要语义：
//! 1. `session/follow` 的 `snapshot` 是**整窗替换**（不是增量），重连时必须用它覆盖本地窗口；
//! 2. durable 事件的 `surfaceOp: { op: "replace", startSeq, endSeq }` 表示
//!    「用本事件替换区间」，compaction 依赖这一语义，不能简单 append。

use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// 会话标识。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for SessionId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for SessionId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// 会话元信息（线上 `SessionWireHeader`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionWireHeader {
    pub version: u32,
    pub id: SessionId,
    pub created_at: u64,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub parent_session: Option<SessionId>,
    /// 是否包含 fork 继承的前缀。
    pub is_seeded: bool,
    #[serde(default)]
    pub origin: Option<String>,
    #[serde(default)]
    pub delegation_depth: Option<u32>,
    #[serde(default)]
    pub agent_preset: Option<String>,
}

/// surface 事件在会话视图上的位置变更。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SurfaceOp {
    /// 追加（显式 `"append"`）。
    Append(String),
    /// 用本事件替换 `[startSeq, endSeq]`（compaction 使用）。
    Replace {
        #[serde(rename = "op")]
        op: String,
        #[serde(rename = "startSeq")]
        start_seq: u64,
        #[serde(rename = "endSeq")]
        end_seq: u64,
    },
}

impl SurfaceOp {
    /// 是否为「替换区间」语义。
    pub fn replacement(&self) -> Option<(u64, u64)> {
        match self {
            SurfaceOp::Replace {
                start_seq, end_seq, ..
            } => Some((*start_seq, *end_seq)),
            SurfaceOp::Append(_) => None,
        }
    }
}

/// 一条 durable 会话事件。
///
/// `data` 的形状由 `type` 决定（上游是 merge-extensible 的 `SessionEventMap`），
/// 因此这里保留原始 JSON，由 state 层按 `type` 解释。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionWireEvent {
    #[serde(rename = "type")]
    pub kind: String,
    pub seq: u64,
    pub time: u64,
    pub data: Value,
    /// 未知类型可安全跳过。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignorable: Option<bool>,
    /// 当前 surface 事件引用的更早事件序号。
    #[serde(
        rename = "sourceEventSeqs",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub source_event_seqs: Option<Value>,
    /// 仅在 surface 事件上出现。
    #[serde(rename = "surfaceOp", default, skip_serializing_if = "Option::is_none")]
    pub surface_op: Option<SurfaceOp>,
}

impl SessionWireEvent {
    /// 未知类型是否允许安全忽略。
    pub fn is_ignorable(&self) -> bool {
        self.ignorable.unwrap_or(false)
    }
}

/// journal 里的一条记录（线上 `SessionEventEntry`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionEventEntry {
    #[serde(rename = "type")]
    pub kind: String,
    pub event: SessionWireEvent,
}

/// 历史页与快照中的记录类型。
pub type SessionHistoryRecord = SessionEventEntry;

/// 会话地址：普通会话或某个 subagent 子会话。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum SessionAddress {
    #[serde(rename = "session")]
    Session {
        #[serde(rename = "sessionId")]
        session_id: SessionId,
    },
    #[serde(rename = "subagent")]
    Subagent {
        #[serde(rename = "parentSessionId")]
        parent_session_id: SessionId,
        #[serde(rename = "childSessionId")]
        child_session_id: SessionId,
        mode: SubagentMode,
    },
}

impl SessionAddress {
    pub fn session(session_id: impl Into<SessionId>) -> Self {
        Self::Session {
            session_id: session_id.into(),
        }
    }

    /// 子会话地址：父会话 + 子会话 + 可继续性。
    pub fn subagent(
        parent_session_id: impl Into<SessionId>,
        child_session_id: impl Into<SessionId>,
        mode: SubagentMode,
    ) -> Self {
        Self::Subagent {
            parent_session_id: parent_session_id.into(),
            child_session_id: child_session_id.into(),
            mode,
        }
    }

    /// 地址指向的会话 id（普通会话或子会话）。
    pub fn session_id(&self) -> &SessionId {
        match self {
            SessionAddress::Session { session_id } => session_id,
            SessionAddress::Subagent {
                child_session_id, ..
            } => child_session_id,
        }
    }

    pub fn parent_session_id(&self) -> Option<&SessionId> {
        match self {
            SessionAddress::Session { .. } => None,
            SessionAddress::Subagent {
                parent_session_id, ..
            } => Some(parent_session_id),
        }
    }

    pub fn mode(&self) -> Option<SubagentMode> {
        match self {
            SessionAddress::Session { .. } => None,
            SessionAddress::Subagent { mode, .. } => Some(*mode),
        }
    }

    pub fn is_subagent(&self) -> bool {
        matches!(self, SessionAddress::Subagent { .. })
    }
}

/// subagent 的生命周期模式。
///
/// 线上是 `one-shot` / `continuable` / `external` / 未知字符串。
/// 未知值统一落到 [`SubagentMode::Unknown`]，避免一个陌生子会话让整棵目录解析失败。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubagentMode {
    OneShot,
    Continuable,
    External,
    Unknown,
}

impl<'de> Deserialize<'de> for SubagentMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.as_str() {
            "one-shot" => Self::OneShot,
            "continuable" => Self::Continuable,
            "external" => Self::External,
            _ => Self::Unknown,
        })
    }
}

impl SubagentMode {
    pub fn as_str(self) -> &'static str {
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
}

/// `session/list` 里的一条会话摘要。
///
/// 上游：`packages/api/session-controller/src/types.ts:151`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    /// 当前是否有存活的 Agent 附着。
    pub agent_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_status: Option<String>,
    pub session_id: SessionId,
    pub updated_at: u64,
    pub running: bool,
    /// 是否是还没开始过的空白会话。
    pub blank: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

/// `session/list` 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionListValue {
    pub items: Vec<SessionSummary>,
}

/// `session/create` 请求。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCreateRequest {
    /// 与 `cwd` 互斥。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// 可复用空白会话，或作为新会话的期望 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_preset: Option<String>,
}

/// `session/create` 响应：实际使用的 id（可能与请求不同）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCreateValue {
    pub session_id: SessionId,
    #[serde(default)]
    pub agent_preset: Option<String>,
}

/// 发送模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptMode {
    /// 排队，等当前 turn 结束。
    Queue,
    /// 打断并插入。
    Steer,
}

/// 用户消息的内容片段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PromptContent {
    Text {
        text: String,
    },
    Image {
        #[serde(rename = "mediaType")]
        media_type: String,
        /// base64。
        data: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// 已上传附件的回执。
    File {
        #[serde(rename = "receiptId")]
        receipt_id: String,
    },
}

impl PromptContent {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }
}

/// `session/prompt` 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPromptRequest {
    /// 客户端铸造；会作为 durable 事件的 `source.rpcId` 回显。
    pub request_id: String,
    pub session_id: SessionId,
    pub mode: PromptMode,
    pub content: Vec<PromptContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_time_zone: Option<String>,
}

/// `session/prompt` 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionPromptValue {
    pub accepted: bool,
}

/// `session/cancel` 请求（中断当前 turn；上游没有独立的 abort 接口）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCancelRequest {
    pub session_id: SessionId,
}

/// 分页窗口。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnWindow {
    pub min_messages: u32,
    pub min_turns: u32,
}

impl Default for TurnWindow {
    /// 上游客户端默认：`{ minMessages: 50, minTurns: 2 }`。
    fn default() -> Self {
        Self {
            min_messages: 50,
            min_turns: 2,
        }
    }
}

/// `session/follow` 请求（stream）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFollowRequest {
    pub address: SessionAddress,
    /// 请求后台激活。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activate: Option<bool>,
    /// 需要进程内 assistant 流式帧。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assistant_stream: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_messages: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_window: Option<TurnWindow>,
}

impl SessionFollowRequest {
    /// 上游默认打开参数：`activate + assistantStream + maxMessages=500 + turnWindow(50,2)`。
    pub fn open_default(session_id: impl Into<SessionId>) -> Self {
        Self::open_address(SessionAddress::session(session_id))
    }

    /// 用完整地址打开（子会话必须携带 parent + mode）。
    pub fn open_address(address: SessionAddress) -> Self {
        Self {
            address,
            activate: Some(true),
            assistant_stream: Some(true),
            max_messages: Some(500),
            turn_window: Some(TurnWindow::default()),
        }
    }
}

/// `session/page` 请求（一元；向上翻历史，也用于补洞）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPageRequest {
    pub address: SessionAddress,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub through_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_messages: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_window: Option<TurnWindow>,
}

/// `session/page` 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPage {
    pub records: Vec<SessionHistoryRecord>,
    pub has_more: bool,
}

/// projection 基线（`{ asOfSeq, values }`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionProjectionBaseline {
    pub as_of_seq: u64,
    pub values: Value,
}

/// `session/projections` 的返回。
///
/// 上游：`packages/api/session-controller/src/types.ts`（`SessionProjectionsResult`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SessionProjectionsValue {
    /// 可用的 projection 切点。
    Sequenced {
        values: Value,
        #[serde(rename = "asOfSeq")]
        as_of_seq: u64,
    },
    /// 历史需要迁移；只能使用缓存值，不提供切点。
    MigrationRequired {
        #[serde(default)]
        values: Value,
    },
}

impl SessionProjectionsValue {
    pub fn values(&self) -> &Value {
        match self {
            Self::Sequenced { values, .. } | Self::MigrationRequired { values } => values,
        }
    }
}

/// `session/control` 的打开基线：全 Host 每个会话的 projection 快照。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionControlBaseline {
    pub projections: HashMap<String, SessionProjectionBaseline>,
}

/// `session/control` 下行的增量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionProjectionUpdate {
    pub session_id: SessionId,
    pub key: String,
    pub value: Value,
    pub seq: u64,
}

/// `session/control` 下行帧（子 Agent 状态的实时来源）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SessionControlFrame {
    Baseline {
        value: SessionControlBaseline,
    },
    Projection {
        #[serde(rename = "sessionId")]
        session_id: SessionId,
        key: String,
        value: Value,
        seq: u64,
    },
}

impl SessionControlFrame {
    /// 增量帧转为统一结构（baseline 返回 `None`）。
    pub fn update(&self) -> Option<SessionProjectionUpdate> {
        match self {
            Self::Baseline { .. } => None,
            Self::Projection {
                session_id,
                key,
                value,
                seq,
            } => Some(SessionProjectionUpdate {
                session_id: session_id.clone(),
                key: key.clone(),
                value: value.clone(),
                seq: *seq,
            }),
        }
    }
}

/// `session/follow` 下行帧。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionFollowFrame {
    /// 打开时的完整窗口；重连后用它整窗替换。
    #[serde(rename = "snapshot")]
    Snapshot {
        header: SessionWireHeader,
        cursor: u64,
        records: Vec<SessionHistoryRecord>,
        #[serde(rename = "hasMore")]
        has_more: bool,
        projections: SessionProjectionBaseline,
        #[serde(rename = "assistantStream", default)]
        assistant_stream: Option<AssistantStreamBaseline>,
    },
    /// 一条 durable 事件。
    #[serde(rename = "event")]
    Event { event: SessionWireEvent },
    /// 进程内 assistant 流式帧（不进入 durable 记录）。
    #[serde(rename = "assistant-stream")]
    AssistantStream { frame: AssistantStreamFrame },
}

impl SessionFollowFrame {
    /// 事件序号（仅 `event` 帧有）。
    pub fn seq(&self) -> Option<u64> {
        match self {
            SessionFollowFrame::Event { event } => Some(event.seq),
            _ => None,
        }
    }
}

/// 流式 attempt 的打开时基线。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantStreamBaseline {
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_attempt: Option<AssistantStreamAttempt>,
}

/// 一个正在进行的 attempt。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantStreamAttempt {
    pub attempt_id: String,
    /// 该 attempt 开始时最后一个 durable 序号。
    pub started_after_seq: u64,
    pub turn: u32,
    pub step: u32,
    /// 下一帧期望的稠密序号。
    pub next_index: u64,
    /// 打开时已累积的紧凑 stream。
    #[serde(default)]
    pub stream: Vec<Value>,
}

/// 流式帧。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AssistantStreamFrame {
    Start {
        #[serde(rename = "attemptId")]
        attempt_id: String,
        revision: u64,
        #[serde(rename = "startedAfterSeq")]
        started_after_seq: u64,
        turn: u32,
        step: u32,
    },
    Chunk {
        #[serde(rename = "attemptId")]
        attempt_id: String,
        revision: u64,
        index: u64,
        time: u64,
        chunk: Value,
    },
    End {
        #[serde(rename = "attemptId")]
        attempt_id: String,
        revision: u64,
        /// 本终结标记代表的 chunk 数。
        index: u64,
        outcome: AssistantStreamEnd,
    },
}

/// 流式结束的两种结局。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum AssistantStreamEnd {
    /// 已提交为 durable 事件。
    Committed {
        #[serde(rename = "eventType")]
        event_type: String,
        seq: u64,
    },
    /// 被丢弃（例如被打断）。
    Abandoned,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn header() -> Value {
        json!({
            "version": 1,
            "id": "s1",
            "createdAt": 1_700_000_000_000u64,
            "cwd": "/home/me/proj",
            "isSeeded": false,
            "agentPreset": "standard"
        })
    }

    #[test]
    fn snapshot_frame_parses() {
        let raw = json!({
            "type": "snapshot",
            "header": header(),
            "cursor": 12,
            "records": [
                { "type": "event", "event": {
                    "type": "turn/start", "seq": 1, "time": 1, "data": { "turn": 1 }
                }}
            ],
            "hasMore": true,
            "projections": { "asOfSeq": 12, "values": {} },
            "assistantStream": { "revision": 3 }
        });
        let frame: SessionFollowFrame = serde_json::from_value(raw).unwrap();
        match frame {
            SessionFollowFrame::Snapshot {
                header,
                cursor,
                records,
                has_more,
                assistant_stream,
                ..
            } => {
                assert_eq!(header.id.as_str(), "s1");
                assert_eq!(cursor, 12);
                assert_eq!(records.len(), 1);
                assert!(has_more);
                assert_eq!(assistant_stream.unwrap().revision, 3);
            }
            other => panic!("期望 snapshot，实际 {other:?}"),
        }
    }

    #[test]
    fn event_frame_with_surface_replace() {
        let raw = json!({
            "type": "event",
            "event": {
                "type": "user/message",
                "seq": 40,
                "time": 123,
                "data": { "text": "summary" },
                "surfaceOp": { "op": "replace", "startSeq": 10, "endSeq": 39 }
            }
        });
        let frame: SessionFollowFrame = serde_json::from_value(raw).unwrap();
        let event = match frame {
            SessionFollowFrame::Event { event } => event,
            other => panic!("期望 event，实际 {other:?}"),
        };
        assert_eq!(event.seq, 40);
        assert_eq!(event.surface_op.unwrap().replacement(), Some((10, 39)));
    }

    #[test]
    fn unknown_event_is_marked_ignorable() {
        let event: SessionWireEvent = serde_json::from_value(json!({
            "type": "plugin/whatever",
            "seq": 41,
            "time": 124,
            "data": {},
            "ignorable": true
        }))
        .unwrap();
        assert!(event.is_ignorable());
        assert_eq!(event.kind, "plugin/whatever");
    }

    #[test]
    fn assistant_stream_frames() {
        let start: SessionFollowFrame = serde_json::from_value(json!({
            "type": "assistant-stream",
            "frame": { "type": "start", "attemptId": "a1", "revision": 1,
                       "startedAfterSeq": 40, "turn": 2, "step": 1 }
        }))
        .unwrap();
        assert!(matches!(
            start,
            SessionFollowFrame::AssistantStream {
                frame: AssistantStreamFrame::Start { .. }
            }
        ));

        let end: SessionFollowFrame = serde_json::from_value(json!({
            "type": "assistant-stream",
            "frame": { "type": "end", "attemptId": "a1", "revision": 1, "index": 7,
                       "outcome": { "kind": "committed", "eventType": "assistant/message", "seq": 41 } }
        }))
        .unwrap();
        match end {
            SessionFollowFrame::AssistantStream {
                frame: AssistantStreamFrame::End { outcome, .. },
            } => assert!(matches!(
                outcome,
                AssistantStreamEnd::Committed { seq: 41, .. }
            )),
            other => panic!("期望 end，实际 {other:?}"),
        }
    }

    #[test]
    fn follow_request_matches_upstream_args() {
        let request = SessionFollowRequest::open_default(SessionId::new("s1"));
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "address": { "kind": "session", "sessionId": "s1" },
                "activate": true,
                "assistantStream": true,
                "maxMessages": 500,
                "turnWindow": { "minMessages": 50, "minTurns": 2 }
            })
        );
    }

    #[test]
    fn prompt_request_matches_upstream_args() {
        let request = SessionPromptRequest {
            request_id: "req-1".into(),
            session_id: SessionId::new("s1"),
            mode: PromptMode::Queue,
            content: vec![PromptContent::text("hello")],
            client_time_zone: Some("Asia/Shanghai".into()),
        };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "requestId": "req-1",
                "sessionId": "s1",
                "mode": "queue",
                "content": [{ "type": "text", "text": "hello" }],
                "clientTimeZone": "Asia/Shanghai"
            })
        );
    }

    #[test]
    fn create_request_omits_absent_fields() {
        let request = SessionCreateRequest {
            cwd: Some("/home/me/proj".into()),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({ "cwd": "/home/me/proj" })
        );
    }

    #[test]
    fn subagent_follow_address_keeps_parent_and_mode() {
        let request = SessionFollowRequest::open_address(SessionAddress::subagent(
            SessionId::new("parent"),
            SessionId::new("child"),
            SubagentMode::Continuable,
        ));
        assert_eq!(
            serde_json::to_value(&request).unwrap()["address"],
            json!({
                "kind": "subagent",
                "parentSessionId": "parent",
                "childSessionId": "child",
                "mode": "continuable"
            })
        );
        assert_eq!(request.address.session_id().as_str(), "child");
        assert_eq!(
            request.address.parent_session_id().map(SessionId::as_str),
            Some("parent")
        );
        assert!(request.address.is_subagent());
    }

    #[test]
    fn projections_value_parses_sequenced_and_migration() {
        let sequenced: SessionProjectionsValue = serde_json::from_value(json!({
            "kind": "sequenced",
            "asOfSeq": 42,
            "values": { "title": "hello" }
        }))
        .unwrap();
        assert_eq!(sequenced.values()["title"], "hello");

        let migration: SessionProjectionsValue = serde_json::from_value(json!({
            "kind": "migration-required",
            "values": { "title": "cached" }
        }))
        .unwrap();
        assert_eq!(migration.values()["title"], "cached");
    }

    #[test]
    fn control_frames_parse_baseline_and_projection() {
        let baseline: SessionControlFrame = serde_json::from_value(json!({
            "type": "baseline",
            "value": {
                "projections": {
                    "s1": { "asOfSeq": 3, "values": { "title": "parent" } }
                }
            }
        }))
        .unwrap();
        match baseline {
            SessionControlFrame::Baseline { value } => {
                assert_eq!(value.projections["s1"].values["title"], "parent");
            }
            other => panic!("期望 baseline，实际 {other:?}"),
        }

        let update: SessionControlFrame = serde_json::from_value(json!({
            "type": "projection",
            "sessionId": "child",
            "key": "subagentTiming",
            "value": { "settledMs": 12 },
            "seq": 9
        }))
        .unwrap();
        let update = update.update().expect("projection 帧应有增量");
        assert_eq!(update.session_id.as_str(), "child");
        assert_eq!(update.key, "subagentTiming");
        assert_eq!(update.seq, 9);
    }
}

/// `session/rename` 请求。上游 `SessionRenameRequest`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRenameRequest {
    pub session_id: SessionId,
    pub title: String,
}

/// `session/rename` 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRenameValue {
    pub title: String,
    pub seq: u64,
}

/// 一个可选项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// 一个提问。上游 `AskUserQuestionItem`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionItem {
    pub id: String,
    pub question: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<QuestionOption>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_select: Option<bool>,
}

/// 单个问题的回答。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionAnswerItem {
    pub id: String,
    pub selected: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom: Option<String>,
}

/// 整批回答。上游 `AskUserQuestionAnswer`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionAnswer {
    pub answers: Vec<QuestionAnswerItem>,
}

/// 队列项上的操作。上游 `packages/api/session-controller/src/types.ts:141`
///
/// ```ts
/// { kind: 'edit', content: TextBlock[] } | { kind: 'remove' } | { kind: 'steer' }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum QueueAction {
    /// 用新的文本内容替换待发消息。
    Edit { content: Vec<PromptContent> },
    /// 从队列移除。
    Remove,
    /// 立即插入（等同打断后发送）。
    Steer,
}

/// `session/updateQueue` 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdateQueueRequest {
    pub session_id: SessionId,
    pub item_id: String,
    pub action: QueueAction,
}

/// `session/updateQueue` 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionUpdateQueueValue {
    pub accepted: bool,
}
