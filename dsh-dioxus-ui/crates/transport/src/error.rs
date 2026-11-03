//! 传输层错误。刻意把「业务错误」与「传输错误」分开：
//! 上游的业务失败是 HTTP 200 + `result.ok=false`，不应被当成网络故障处理。

use dsh_proto::RpcError;

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// 拉起 `dsh` 失败，或启动期没有打印可用地址。
    #[error("Host 启动失败: {0}")]
    HostStartup(String),

    #[error("HTTP 请求失败: {0}")]
    Http(#[from] reqwest::Error),

    #[error("IO 失败: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 编解码失败: {0}")]
    Json(#[from] serde_json::Error),

    #[error("WebSocket 失败: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    /// 帧/信封不符合协议（本地解析或对端返回异常形状）。
    #[error("协议错误: {0}")]
    Protocol(String),

    /// Host 明确返回的业务错误。
    #[error("业务错误: {0}")]
    Rpc(#[from] RpcError),

    /// 连接或通道已关闭。
    #[error("连接已关闭")]
    Closed,

    #[error("超时: {0}")]
    Timeout(String),
}

impl TransportError {
    /// 是否为「连接层面失效」——调用方据此触发重连，而不是提示用户。
    pub fn is_connection_lost(&self) -> bool {
        matches!(
            self,
            TransportError::WebSocket(_) | TransportError::Closed | TransportError::Io(_)
        )
    }
}
