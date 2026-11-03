//! 把传输层与会话状态编排成一个可直接用的客户端。
//!
//! 职责边界：
//! - `dsh-transport` 只知道「怎么发帧」，不知道「会话」；
//! - `dsh-state` 只知道「怎么归约」，不知道「从哪拿帧」；
//! - 这里负责把两者接起来：启动 Host → 取凭据 → 打开流 → 喂给 `SessionView`。
//!
//! 关键约定（均已在真实 Host 上验证）：
//! - `session/list` 的 `args` 是 `{ "_request": {} }`；
//! - 其余会话方法统一是 `{ "request": … }`；
//! - `$events` 的 `ready` 帧是连接代次来源，`clientId` 用于 waterfall 应答。

use std::time::Duration;

use dsh_proto::endpoint::{
    API_PREFIX, files as files_ep, session as session_ep, subagent as subagent_ep,
};
use dsh_proto::events::{
    ApprovalOutcome, EventOutcome, REMOTE_EVENT_RESULT_ENDPOINT, REMOTE_EVENT_STREAM_ENDPOINT,
    RemoteEventFrame, RemoteEventResult,
};
use dsh_proto::session::{
    PromptContent, PromptMode, QueueAction, SessionAddress, SessionCreateRequest,
    SessionCreateValue, SessionFollowFrame, SessionFollowRequest, SessionListValue,
    SessionProjectionsValue, SessionPromptRequest, SessionPromptValue, SessionSummary,
    SessionUpdateQueueRequest, SubagentMode,
};
use dsh_proto::subagent::{
    SubagentInterruptRequest, SubagentInterruptValue, SubagentPromptRequest, SubagentPromptValue,
};
use dsh_state::SessionView;
use dsh_transport::{
    Credential, HostConfig, HostSupervisor, MuxConnection, RpcClient, StreamEvent, StreamHandle,
    TransportError,
};
use serde_json::json;

/// 一个已连上的 Host 客户端。
pub struct DshClient {
    host: HostSupervisor,
    rpc: RpcClient,
    mux: MuxConnection,
}

impl DshClient {
    /// 拉起 Host 并完成握手。
    pub async fn launch() -> Result<Self, TransportError> {
        let host = HostSupervisor::spawn(HostConfig::default()).await?;
        Self::attach(host).await
    }

    /// 复用一个已经启动的 Host（便于测试与「Host 已在运行」场景）。
    pub async fn attach(host: HostSupervisor) -> Result<Self, TransportError> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .build()?;

        let credential = Credential::acquire(&http, host.base_url(), host.token()).await?;
        let rpc = RpcClient::new(
            host.base_url().clone(),
            Some(&credential),
            Duration::from_secs(30),
        )?;
        let mux = MuxConnection::connect(host.base_url(), Some(&credential)).await?;

        Ok(Self { host, rpc, mux })
    }

    /// 底层 RPC 客户端（探测/诊断用）。
    pub fn rpc(&self) -> &RpcClient {
        &self.rpc
    }

    pub fn base_url(&self) -> String {
        self.host.base_url().to_string()
    }

    /// 会话列表。
    pub async fn list_sessions(&self) -> Result<Vec<SessionSummary>, TransportError> {
        // 注意 `_request` —— 上游形参名，写错会被 gateway 拒绝
        let value = self
            .rpc
            .call::<SessionListValue>(session_ep::LIST, json!({ "_request": {} }))
            .await?;
        Ok(value.map(|list| list.items).unwrap_or_default())
    }

    /// 新建（或复用）会话。
    pub async fn create_session(
        &self,
        cwd: Option<String>,
    ) -> Result<SessionCreateValue, TransportError> {
        let request = SessionCreateRequest {
            cwd,
            ..Default::default()
        };
        self.rpc
            .call::<SessionCreateValue>(session_ep::CREATE, json!({ "request": request }))
            .await?
            .ok_or_else(|| TransportError::Protocol("session/create 未返回结果".into()))
    }

    /// 发送一条文本消息（默认排队）。
    pub async fn prompt(
        &self,
        session_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<(), TransportError> {
        self.prompt_with_mode(session_id, text, PromptMode::Queue)
            .await
    }

    /// 发送一条文本消息，显式指定 `queue` 或 `steer`。
    ///
    /// `steer` 会打断当前 turn 并插入，界面上体现为「插话」。
    pub async fn prompt_with_mode(
        &self,
        session_id: impl Into<String>,
        text: impl Into<String>,
        mode: PromptMode,
    ) -> Result<(), TransportError> {
        let request = SessionPromptRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            session_id: dsh_proto::session::SessionId::new(session_id),
            mode,
            content: vec![PromptContent::text(text)],
            client_time_zone: None,
        };
        self.rpc
            .call::<SessionPromptValue>(session_ep::PROMPT, json!({ "request": request }))
            .await?
            .ok_or_else(|| TransportError::Protocol("session/prompt 未返回结果".into()))?;
        Ok(())
    }

    /// 中断当前 turn。
    pub async fn cancel(&self, session_id: impl Into<String>) -> Result<(), TransportError> {
        let request = json!({ "request": { "sessionId": session_id.into() } });
        self.rpc
            .call_value(session_ep::CANCEL, request)
            .await
            .map(|_| ())
    }

    /// 打开会话事件流（快照 + 增量 + 流式）。
    pub fn follow(&self, session_id: impl Into<String>) -> Result<StreamHandle, TransportError> {
        self.follow_address(SessionAddress::session(session_id.into()))
    }

    /// 用完整地址打开会话流；子会话必须携带 parent + mode。
    pub fn follow_address(&self, address: SessionAddress) -> Result<StreamHandle, TransportError> {
        let request = SessionFollowRequest::open_address(address);
        self.mux
            .open_args(session_ep::FOLLOW, json!({ "request": request }))
    }

    /// 打开 `workspace/follow`（工作区与会话归属的基线 + 增量）。
    pub fn follow_workspace(&self) -> Result<StreamHandle, TransportError> {
        self.mux
            .open_args(dsh_proto::endpoint::workspace::FOLLOW, json!({}))
    }

    /// 从 `workspace/follow` 的 baseline 帧里取归档会话 id。
    ///
    /// 实测形状：`{"type":"baseline","value":{"archivedSessionIds":[..],"items":[],"pinnedSessionIds":[..]}}`
    /// —— `session/list` 仍会返回归档会话，需要用它过滤。
    pub fn archived_ids(value: &serde_json::Value) -> Vec<String> {
        value
            .get("value")
            .and_then(|value| value.get("archivedSessionIds"))
            .and_then(serde_json::Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 打开 `$events`（连接代次与审批 waterfall 的来源）。
    pub fn open_events(&self) -> Result<StreamHandle, TransportError> {
        self.mux.open_args(REMOTE_EVENT_STREAM_ENDPOINT, json!({}))
    }

    /// 打开 `session/control`（全 Host projection 基线 + 增量）。
    pub fn open_control(&self) -> Result<StreamHandle, TransportError> {
        self.mux.open_args(session_ep::CONTROL, json!({}))
    }

    /// 按需刷新一个会话的 projection（子目录展开 / 失败重试）。
    pub async fn refresh_projections(
        &self,
        session_id: impl Into<String>,
    ) -> Result<SessionProjectionsValue, TransportError> {
        self.rpc
            .call::<SessionProjectionsValue>(
                session_ep::PROJECTIONS,
                json!({ "request": { "sessionId": session_id.into() } }),
            )
            .await?
            .ok_or_else(|| TransportError::Protocol("session/projections 未返回结果".into()))
    }

    /// 向 continuable 子会话续聊。返回被接受的消息 id。
    pub async fn prompt_subagent(
        &self,
        parent_session_id: impl Into<String>,
        child_session_id: impl Into<String>,
        mode: SubagentMode,
        text: impl Into<String>,
        delivery: PromptMode,
    ) -> Result<String, TransportError> {
        let request = SubagentPromptRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            parent_session_id: dsh_proto::session::SessionId::new(parent_session_id),
            child_session_id: dsh_proto::session::SessionId::new(child_session_id),
            mode,
            delivery,
            content: vec![PromptContent::text(text)],
            client_time_zone: None,
        };
        let value = self
            .rpc
            .call::<SubagentPromptValue>(subagent_ep::PROMPT, json!({ "request": request }))
            .await?
            .ok_or_else(|| TransportError::Protocol("subagent/prompt 未返回结果".into()))?;
        Ok(value.message_id)
    }

    /// 独立停止 continuable 子会话的当前 turn（不动父会话）。
    pub async fn interrupt_subagent(
        &self,
        parent_session_id: impl Into<String>,
        child_session_id: impl Into<String>,
        mode: SubagentMode,
    ) -> Result<bool, TransportError> {
        let request = SubagentInterruptRequest {
            child_session_id: dsh_proto::session::SessionId::new(child_session_id),
            parent_session_id: dsh_proto::session::SessionId::new(parent_session_id),
            mode,
        };
        let value = self
            .rpc
            .call::<SubagentInterruptValue>(
                subagent_ep::INTERRUPT_BY_PARENT,
                serde_json::to_value(request)?,
            )
            .await?
            .ok_or_else(|| {
                TransportError::Protocol("subagent/interruptByParent 未返回结果".into())
            })?;
        Ok(value.accepted)
    }

    /// 把一条流的帧喂给 `SessionView`，返回本次消费的帧数。
    ///
    /// `max_frames` 用于测试/示例里做有界消费；生产环境应由调用方驱动。
    pub async fn pump_into(
        &self,
        stream: &mut StreamHandle,
        view: &mut SessionView,
        max_frames: usize,
        timeout: Duration,
    ) -> usize {
        let mut consumed = 0;
        while consumed < max_frames {
            let next = tokio::time::timeout(timeout, stream.next()).await;
            match next {
                Ok(Some(StreamEvent::Frame(value))) => {
                    match serde_json::from_value::<SessionFollowFrame>(value) {
                        Ok(frame) => {
                            view.apply(frame);
                            consumed += 1;
                        }
                        Err(err) => tracing::warn!(%err, "无法解析 follow 帧"),
                    }
                }
                Ok(Some(StreamEvent::Error(err))) => {
                    tracing::warn!(%err, "follow 流出错");
                    break;
                }
                Ok(Some(StreamEvent::End)) | Ok(None) => break,
                Err(_) => break,
            }
        }
        consumed
    }

    /// 读取 `$events` 的 `ready` 帧，返回 `clientId`。
    pub async fn await_ready(&self, stream: &mut StreamHandle) -> Option<String> {
        let next = tokio::time::timeout(Duration::from_secs(10), stream.next()).await;
        match next {
            Ok(Some(StreamEvent::Frame(value))) => {
                match serde_json::from_value::<RemoteEventFrame>(value) {
                    Ok(RemoteEventFrame::Ready { client_id, .. }) => Some(client_id),
                    Ok(other) => {
                        tracing::warn!(?other, "首个事件帧不是 ready");
                        None
                    }
                    Err(err) => {
                        tracing::warn!(%err, "无法解析事件帧");
                        None
                    }
                }
            }
            _ => None,
        }
    }

    /// 应答一个 waterfall（审批/提问）。
    ///
    /// 注意 `$events/result` 的 `args` 是平铺的 `{clientId, eventId, outcome}`，
    /// 没有 `request` 外壳；`clientId` 必须用当前 generation 的（重连后旧 id 失效）。
    pub async fn respond_event(
        &self,
        client_id: impl Into<String>,
        event_id: impl Into<String>,
        outcome: EventOutcome,
    ) -> Result<(), TransportError> {
        let result = RemoteEventResult::new(client_id, event_id, outcome);
        self.rpc
            .call_value(REMOTE_EVENT_RESULT_ENDPOINT, serde_json::to_value(result)?)
            .await
            .map(|_| ())
    }

    /// 审批快捷方法：`allowed-once` / `rejected`。
    pub async fn answer_approval(
        &self,
        client_id: impl Into<String>,
        event_id: impl Into<String>,
        allow: bool,
    ) -> Result<(), TransportError> {
        let outcome = EventOutcome::approval(if allow {
            ApprovalOutcome::AllowedOnce
        } else {
            ApprovalOutcome::Rejected
        });
        self.respond_event(client_id, event_id, outcome).await
    }

    /// 关闭 Host 子进程。
    pub async fn shutdown(self) {
        self.host.shutdown().await;
    }

    /// API 前缀（诊断用）。
    pub fn api_prefix(&self) -> &'static str {
        API_PREFIX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 审批回执的线上形状（`$events/result` 的 args 是平铺字段）。
    #[test]
    fn approval_result_shape() {
        let result = RemoteEventResult::new(
            "client-1",
            "ev-1",
            EventOutcome::approval(ApprovalOutcome::AllowedOnce),
        );
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({
                "clientId": "client-1",
                "eventId": "ev-1",
                "outcome": { "kind": "result", "value": "allowed-once" }
            })
        );
    }

    /// 只验证 args 形状，不触网。
    #[test]
    fn list_args_use_request_underscore() {
        let args = json!({ "_request": {} });
        assert!(args.get("_request").is_some());
        assert!(args.get("request").is_none(), "写错键名会被 gateway 拒绝");
    }
}

/// 文件树的一项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileItem {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
}

impl DshClient {
    /// 列目录。`scope_id` 线上叫 `workspaceFileScopeId`（值就是 sessionId）。
    ///
    /// 返回结构上游未在文档里固定，这里做宽容解析：支持数组或 `{items:[…]}`，
    /// 目录判定兼容 `isDir` / `type` / `kind` 三种写法。
    pub async fn list_files(
        &self,
        scope_id: &str,
        path: &str,
    ) -> Result<Vec<FileItem>, TransportError> {
        let value = self
            .rpc
            .call_value(
                files_ep::LIST,
                json!({ "workspaceFileScopeId": scope_id, "path": path }),
            )
            .await?;

        let Some(value) = value else {
            return Ok(Vec::new());
        };

        let array = value
            .get("items")
            .and_then(serde_json::Value::as_array)
            .or_else(|| value.get("entries").and_then(serde_json::Value::as_array))
            .or_else(|| value.as_array());

        let Some(array) = array else {
            return Ok(Vec::new());
        };

        Ok(array
            .iter()
            .filter_map(|entry| {
                let path = entry
                    .get("path")
                    .or_else(|| entry.get("name"))
                    .and_then(serde_json::Value::as_str)?;
                let name = entry
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path));
                let is_dir = entry
                    .get("isDir")
                    .and_then(serde_json::Value::as_bool)
                    .or_else(|| {
                        entry
                            .get("isDirectory")
                            .and_then(serde_json::Value::as_bool)
                    })
                    .unwrap_or_else(|| {
                        matches!(
                            entry
                                .get("type")
                                .or_else(|| entry.get("kind"))
                                .and_then(serde_json::Value::as_str),
                            Some("directory") | Some("dir") | Some("folder")
                        )
                    });

                Some(FileItem {
                    path: path.to_string(),
                    name: name.to_string(),
                    is_dir,
                })
            })
            .collect())
    }

    /// 读取文本文件（预览用）。
    pub async fn read_file(&self, scope_id: &str, path: &str) -> Result<String, TransportError> {
        let value = self
            .rpc
            .call_value(
                files_ep::READ,
                json!({ "workspaceFileScopeId": scope_id, "path": path }),
            )
            .await?;

        let Some(value) = value else {
            return Ok(String::new());
        };

        if let Some(text) = value.as_str() {
            return Ok(text.to_string());
        }
        for key in ["text", "content", "data"] {
            if let Some(text) = value.get(key).and_then(serde_json::Value::as_str) {
                return Ok(text.to_string());
            }
        }
        Ok(value.to_string())
    }
}

impl DshClient {
    /// 拉一页更早的历史（`session/page`，`beforeSeq` 不含）。
    pub async fn page_before(
        &self,
        session_id: impl Into<String>,
        before_seq: u64,
        max_messages: u32,
    ) -> Result<dsh_proto::session::SessionPage, TransportError> {
        self.page_before_address(
            SessionAddress::session(session_id.into()),
            before_seq,
            max_messages,
        )
        .await
    }

    /// 用完整地址翻历史（子会话必须带 parent + mode）。
    pub async fn page_before_address(
        &self,
        address: SessionAddress,
        before_seq: u64,
        max_messages: u32,
    ) -> Result<dsh_proto::session::SessionPage, TransportError> {
        let request = dsh_proto::session::SessionPageRequest {
            address,
            through_seq: None,
            before_seq: Some(before_seq),
            max_messages: Some(max_messages),
            turn_window: Some(dsh_proto::session::TurnWindow::default()),
        };
        self.rpc
            .call::<dsh_proto::session::SessionPage>(
                session_ep::PAGE,
                json!({ "request": request }),
            )
            .await?
            .ok_or_else(|| TransportError::Protocol("session/page 未返回结果".into()))
    }

    /// 修改队列项。形状已对真实 Host 验证：
    /// `{request:{sessionId,itemId,action:{kind:"remove"|"steer"|"edit",content?}}}`
    /// —— 合法形状会走到业务错误 `session/queue-item-not-found`，非法形状则是
    /// `gateway/input-invalid`。
    pub async fn update_queue(
        &self,
        session_id: impl Into<String>,
        item_id: impl Into<String>,
        action: QueueAction,
    ) -> Result<(), TransportError> {
        let request = SessionUpdateQueueRequest {
            session_id: dsh_proto::session::SessionId::new(session_id),
            item_id: item_id.into(),
            action,
        };
        self.rpc
            .call_value(session_ep::UPDATE_QUEUE, json!({ "request": request }))
            .await
            .map(|_| ())
    }

    /// 从队列移除一条待发消息。
    pub async fn remove_queued(
        &self,
        session_id: impl Into<String>,
        item_id: impl Into<String>,
    ) -> Result<(), TransportError> {
        self.update_queue(session_id, item_id, QueueAction::Remove)
            .await
    }

    /// 把队列项立即插入（打断当前 turn）。
    pub async fn steer_queued(
        &self,
        session_id: impl Into<String>,
        item_id: impl Into<String>,
    ) -> Result<(), TransportError> {
        self.update_queue(session_id, item_id, QueueAction::Steer)
            .await
    }

    /// 回答问题（`userQuestions/answer`）。`answer` 是 `AskUserQuestionAnswer` 的 JSON。
    pub async fn answer_question(
        &self,
        agent_id: impl Into<String>,
        call_id: impl Into<String>,
        answer: serde_json::Value,
    ) -> Result<(), TransportError> {
        self.rpc
            .call_value(
                "userQuestions/answer",
                json!({
                    "agentId": agent_id.into(),
                    "callId": call_id.into(),
                    "answer": answer,
                }),
            )
            .await
            .map(|_| ())
    }

    /// 重命名会话。
    pub async fn rename_session(
        &self,
        session_id: impl Into<String>,
        title: impl Into<String>,
    ) -> Result<(), TransportError> {
        let request = serde_json::json!({
            "request": { "sessionId": session_id.into(), "title": title.into() }
        });
        self.rpc
            .call_value(session_ep::RENAME, request)
            .await
            .map(|_| ())
    }

    /// 归档会话。
    ///
    /// 上游 `workspace/archiveSession` 的 `args` 尚未实测，这里按
    /// `{ "request": { "sessionId": … } }` 发送；失败时由调用方提示。
    pub async fn archive_session(
        &self,
        session_id: impl Into<String>,
    ) -> Result<(), TransportError> {
        let request = serde_json::json!({ "request": { "sessionId": session_id.into() } });
        self.rpc
            .call_value("workspace/archiveSession", request)
            .await
            .map(|_| ())
    }
}
