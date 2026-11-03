//! WebSocket mux（逻辑流多路复用）。
//!
//! 上游出处：
//! - `packages/api/gateway/src/stream-protocol.ts`
//!   （`REMOTE_STREAM_MUX_PATH` / `RemoteStreamClientMessage` / `RemoteStreamServerMessage`）
//!
//! 约定：
//! - 一个物理 WebSocket 承载全部逻辑流，`streamId` 由**客户端**生成；
//! - 帧是 JSON **文本帧**，上游按 exact-keys 校验；
//! - 对已结束的 `streamId` 再发 `item/end/cancel` 会被丢弃，只有重复 `open` 才会关掉 socket。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 唯一的 WebSocket 路由。
pub const REMOTE_STREAM_MUX_PATH: &str = "/api/remote.mux";

/// 客户端 → Host。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum MuxClientMsg {
    /// 打开一条逻辑流；`payload` 恒为 `{ "args": … }`。
    Open {
        #[serde(rename = "streamId")]
        stream_id: String,
        endpoint: String,
        payload: Value,
    },
    /// 上行帧（仅 `terminal/attach` 等带 uplink 的流需要）。
    Item {
        #[serde(rename = "streamId")]
        stream_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<Value>,
    },
    /// 上行半关闭。
    End {
        #[serde(rename = "streamId")]
        stream_id: String,
    },
    /// 取消该流（下行不再投递）。
    Cancel {
        #[serde(rename = "streamId")]
        stream_id: String,
    },
}

impl MuxClientMsg {
    /// 构造 `open` 帧；`args` 需自带 `{ "args": … }` 外壳（见 [`crate::Args`]）。
    pub fn open(stream_id: impl Into<String>, endpoint: impl Into<String>, payload: Value) -> Self {
        Self::Open {
            stream_id: stream_id.into(),
            endpoint: endpoint.into(),
            payload,
        }
    }

    pub fn stream_id(&self) -> &str {
        match self {
            MuxClientMsg::Open { stream_id, .. }
            | MuxClientMsg::Item { stream_id, .. }
            | MuxClientMsg::End { stream_id }
            | MuxClientMsg::Cancel { stream_id } => stream_id,
        }
    }
}

/// Host → 客户端。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum MuxServerMsg {
    Item {
        #[serde(rename = "streamId")]
        stream_id: String,
        #[serde(default)]
        value: Option<Value>,
    },
    Error {
        #[serde(rename = "streamId")]
        stream_id: String,
        error: crate::RpcError,
    },
    End {
        #[serde(rename = "streamId")]
        stream_id: String,
    },
}

impl MuxServerMsg {
    pub fn stream_id(&self) -> &str {
        match self {
            MuxServerMsg::Item { stream_id, .. }
            | MuxServerMsg::Error { stream_id, .. }
            | MuxServerMsg::End { stream_id } => stream_id,
        }
    }
}

/// 二进制上传路由（`content-type: application/octet-stream`，非 RPC 信封）。
pub const UPLOAD_FILE_BINARY_PATH: &str = "/api/session/uploadFileBinary";

/// 文件/图片读取路由（`GET /api/file?path=…&v=…`）。
pub const FILE_BYTES_PATH: &str = "/api/file";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn open_frame_shape() {
        let msg = MuxClientMsg::open(
            "stream-1",
            "session/follow",
            json!({ "args": { "request": { "activate": true } } }),
        );
        assert_eq!(
            serde_json::to_value(&msg).unwrap(),
            json!({
                "type": "open",
                "streamId": "stream-1",
                "endpoint": "session/follow",
                "payload": { "args": { "request": { "activate": true } } }
            })
        );
    }

    #[test]
    fn item_without_value_omits_field() {
        let msg = MuxClientMsg::Item {
            stream_id: "s".into(),
            value: None,
        };
        assert_eq!(
            serde_json::to_value(&msg).unwrap(),
            json!({ "type": "item", "streamId": "s" })
        );
    }

    #[test]
    fn server_item_and_error_parse() {
        let item: MuxServerMsg =
            serde_json::from_value(json!({"type":"item","streamId":"s","value":{"a":1}})).unwrap();
        assert_eq!(item.stream_id(), "s");

        let error: MuxServerMsg = serde_json::from_value(json!({
            "type": "error",
            "streamId": "s",
            "error": { "code": "gateway/protocol", "message": "bad", "details": {} }
        }))
        .unwrap();
        match error {
            MuxServerMsg::Error { error, .. } => assert_eq!(error.code, "gateway/protocol"),
            other => panic!("期望 error，实际 {other:?}"),
        }
    }

    #[test]
    fn end_and_cancel_have_no_extra_fields() {
        let end: MuxServerMsg =
            serde_json::from_value(json!({"type":"end","streamId":"s"})).unwrap();
        assert!(matches!(end, MuxServerMsg::End { .. }));

        let cancel: MuxClientMsg =
            serde_json::from_value(json!({"type":"cancel","streamId":"s"})).unwrap();
        assert!(matches!(cancel, MuxClientMsg::Cancel { .. }));
    }
}
