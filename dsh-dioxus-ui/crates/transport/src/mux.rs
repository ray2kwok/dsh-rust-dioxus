//! WebSocket 逻辑流多路复用。
//!
//! 一条物理连接承载全部逻辑流：`open` 之后 Host 用 `item/error/end` 按 `streamId` 回帧，
//! 客户端用 `cancel` 主动结束。协议细节见 [`dsh_proto::stream`]。
//!
//! 生命周期约定：
//! - 读任务结束时（socket 关闭/协议错），把**所有**未结束的流都标记为 `End`，
//!   由上层决定是否重连；
//! - 重连必须**先丢弃旧 socket**再建新的：v2 cookie 同一时刻只允许一个 WebSocket，
//!   重复连接会被 Host 以 409 拒绝。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use dsh_proto::RpcError;
use dsh_proto::stream::{MuxClientMsg, MuxServerMsg, REMOTE_STREAM_MUX_PATH};
use futures_util::{SinkExt, StreamExt};
use reqwest::Url;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::credential::Credential;
use crate::error::TransportError;

/// 一条流上下发的消息。
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    /// 一帧下行值（`item`）。
    Frame(Value),
    /// Host 报告该流出错（`error`）。
    Error(RpcError),
    /// 流结束（`end` 或连接断开）。
    End,
}

/// 逻辑流句柄。
pub struct StreamHandle {
    stream_id: String,
    rx: mpsc::UnboundedReceiver<StreamEvent>,
    control: mpsc::UnboundedSender<MuxClientMsg>,
}

impl StreamHandle {
    pub fn id(&self) -> &str {
        &self.stream_id
    }

    /// 取下一帧；返回 `None` 表示通道已被丢弃。
    pub async fn next(&mut self) -> Option<StreamEvent> {
        self.rx.recv().await
    }

    /// 主动取消（Host 侧收到后不再投递）。
    pub fn cancel(&self) {
        let _ = self.control.send(MuxClientMsg::Cancel {
            stream_id: self.stream_id.clone(),
        });
    }

    /// 上行一帧（仅带 uplink 的流使用，如 `terminal/attach`）。
    pub fn send_item(&self, value: Value) {
        let _ = self.control.send(MuxClientMsg::Item {
            stream_id: self.stream_id.clone(),
            value: Some(value),
        });
    }

    /// 上行半关闭。
    pub fn end(&self) {
        let _ = self.control.send(MuxClientMsg::End {
            stream_id: self.stream_id.clone(),
        });
    }
}

/// 复用一条 WS 连接的多路复用器。
#[derive(Clone)]
pub struct MuxConnection {
    tx: mpsc::UnboundedSender<MuxClientMsg>,
    streams: Arc<Mutex<HashMap<String, mpsc::UnboundedSender<StreamEvent>>>>,
}

impl MuxConnection {
    /// 建立连接并启动读写任务。
    pub async fn connect(
        base: &Url,
        credential: Option<&Credential>,
    ) -> Result<Self, TransportError> {
        let mut ws_url = base
            .join(REMOTE_STREAM_MUX_PATH.trim_start_matches('/'))
            .map_err(|err| TransportError::Protocol(format!("拼接 WS URL 失败: {err}")))?;
        let scheme = if matches!(base.scheme(), "https" | "wss") {
            "wss"
        } else {
            "ws"
        };
        ws_url
            .set_scheme(scheme)
            .map_err(|_| TransportError::Protocol("无法设置 WS scheme".into()))?;

        let mut request = ws_url
            .as_str()
            .into_client_request()
            .map_err(|err| TransportError::Protocol(format!("构造 WS 请求失败: {err}")))?;

        if let Some(credential) = credential {
            let value = reqwest::header::HeaderValue::from_str(credential.header_value())
                .map_err(|err| TransportError::Protocol(format!("cookie 非法: {err}")))?;
            request.headers_mut().insert("cookie", value);
        }

        let (socket, _response) = tokio_tungstenite::connect_async(request).await?;
        let (mut sink, mut source) = socket.split();

        let (tx, mut rx) = mpsc::unbounded_channel::<MuxClientMsg>();
        let streams: Arc<Mutex<HashMap<String, mpsc::UnboundedSender<StreamEvent>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // 写任务
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                let text = match serde_json::to_string(&message) {
                    Ok(text) => text,
                    Err(err) => {
                        tracing::error!(%err, "序列化 mux 帧失败");
                        continue;
                    }
                };
                if let Err(err) = sink.send(Message::Text(text.into())).await {
                    tracing::warn!(%err, "mux 发送失败，停止写任务");
                    break;
                }
            }
        });

        // 读任务
        let reader_streams = Arc::clone(&streams);
        tokio::spawn(async move {
            while let Some(message) = source.next().await {
                let text = match message {
                    Ok(Message::Text(text)) => text,
                    Ok(Message::Close(_)) => break,
                    Ok(_) => continue,
                    Err(err) => {
                        tracing::warn!(%err, "mux 读取失败");
                        break;
                    }
                };

                match serde_json::from_str::<MuxServerMsg>(text.as_str()) {
                    Ok(MuxServerMsg::Item { stream_id, value }) => {
                        dispatch(&reader_streams, &stream_id, |tx| {
                            let _ = tx.send(StreamEvent::Frame(value.unwrap_or(Value::Null)));
                        });
                    }
                    Ok(MuxServerMsg::Error { stream_id, error }) => {
                        dispatch(&reader_streams, &stream_id, |tx| {
                            let _ = tx.send(StreamEvent::Error(error));
                        });
                    }
                    Ok(MuxServerMsg::End { stream_id }) => {
                        dispatch(&reader_streams, &stream_id, |tx| {
                            let _ = tx.send(StreamEvent::End);
                        });
                    }
                    Err(err) => {
                        tracing::warn!(%err, "无法解析 mux 帧，已忽略");
                    }
                }
            }

            // 连接结束：让所有流看到 End，上层据此决定是否重连
            let leftover: Vec<_> = {
                let mut guard = reader_streams.lock().unwrap();
                guard.drain().map(|(_, tx)| tx).collect()
            };
            for tx in leftover {
                let _ = tx.send(StreamEvent::End);
            }
        });

        Ok(Self { tx, streams })
    }

    /// 打开一条逻辑流；`args` 需自带 `{ "args": … }` 外壳。
    pub fn open(&self, endpoint: &str, args: Value) -> Result<StreamHandle, TransportError> {
        let stream_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::unbounded_channel();

        self.streams.lock().unwrap().insert(stream_id.clone(), tx);

        self.tx
            .send(MuxClientMsg::open(stream_id.clone(), endpoint, args))
            .map_err(|_| TransportError::Closed)?;

        Ok(StreamHandle {
            stream_id,
            rx,
            control: self.tx.clone(),
        })
    }

    /// 当前活跃流数量（诊断用）。
    pub fn active_streams(&self) -> usize {
        self.streams.lock().unwrap().len()
    }
}

fn dispatch(
    streams: &Arc<Mutex<HashMap<String, mpsc::UnboundedSender<StreamEvent>>>>,
    stream_id: &str,
    action: impl FnOnce(&mpsc::UnboundedSender<StreamEvent>),
) {
    let sender = {
        let guard = streams.lock().unwrap();
        guard.get(stream_id).cloned()
    };
    if let Some(sender) = sender {
        action(&sender);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_url_matches_upstream_path() {
        let base = Url::parse("http://127.0.0.1:3080/").unwrap();
        let ws = base
            .join(REMOTE_STREAM_MUX_PATH.trim_start_matches('/'))
            .unwrap();
        assert_eq!(ws.as_str(), "http://127.0.0.1:3080/api/remote.mux");
        assert_eq!(REMOTE_STREAM_MUX_PATH, "/api/remote.mux");
    }
}
