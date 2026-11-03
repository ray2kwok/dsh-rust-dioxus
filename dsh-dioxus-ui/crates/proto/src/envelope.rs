//! 一元 RPC 信封。
//!
//! 上游出处：
//! - `packages/client/connection/src/rpc.ts`（`ClientRequest` / `ServerResponse` / `ConnectionRpcResult`）
//! - `packages/client/connection/src/rpc-schema.ts`（zod 校验）
//!
//! 三个容易踩的点：
//! 1. `payload` 恒为 `{ "args": { … } }`，且 `args` 的键名是 **Host 侧形参名**；
//! 2. 业务失败仍是 **HTTP 200**，错误在 `result.error` 里；
//! 3. void 结果时 `value` **字段缺省**（不是 `null`）。

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// 请求信封的 `type` 判别值。
pub const TYPE_CLIENT_REQUEST: &str = "client-request";
/// 响应信封的 `type` 判别值。
pub const TYPE_SERVER_RESPONSE: &str = "server-response";

/// 一元 RPC 请求信封。
///
/// ```json
/// { "type": "client-request", "rpcId": "…", "method": "session/prompt",
///   "payload": { "args": { "request": { … } } } }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientRequest<P> {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "rpcId")]
    pub rpc_id: String,
    pub method: String,
    pub payload: P,
}

impl<P> ClientRequest<P> {
    /// 构造请求；`kind` 恒为 [`TYPE_CLIENT_REQUEST`]。
    pub fn new(rpc_id: impl Into<String>, method: impl Into<String>, payload: P) -> Self {
        Self {
            kind: TYPE_CLIENT_REQUEST.to_string(),
            rpc_id: rpc_id.into(),
            method: method.into(),
            payload,
        }
    }
}

/// `payload` 的唯一合法形状：`{ "args": … }`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Args<V = serde_json::Value> {
    pub args: V,
}

impl<V> Args<V> {
    pub fn new(args: V) -> Self {
        Self { args }
    }
}

/// 一元 RPC 响应信封。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerResponse<T> {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "rpcId")]
    pub rpc_id: String,
    pub result: RpcResult<T>,
}

impl<T> ServerResponse<T> {
    pub fn is_ok(&self) -> bool {
        matches!(self.result, RpcResult::Ok { .. })
    }
}

/// 业务错误。`details` 在线上总是存在（可能为空对象），这里容忍缺省。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {message}")]
pub struct RpcError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub details: serde_json::Value,
}

/// RPC 结果。`Ok` 的载荷是 `Option<T>`：协议允许 void 结果缺省 `value`。
#[derive(Debug, Clone, PartialEq)]
pub enum RpcResult<T> {
    Ok { value: Option<T> },
    Failed { error: RpcError },
}

impl<T> RpcResult<T> {
    /// 取出成功值；void 结果返回 `None`。
    pub fn ok(self) -> Option<Option<T>> {
        match self {
            RpcResult::Ok { value } => Some(value),
            RpcResult::Failed { .. } => None,
        }
    }

    pub fn error(self) -> Option<RpcError> {
        match self {
            RpcResult::Ok { .. } => None,
            RpcResult::Failed { error } => Some(error),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for RpcResult<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // 注意：`Option<T>` 字段在 serde 中天然可选，不要再加 `#[serde(default)]`，
        // 否则 derive 会为泛型参数引入 `T: Default` 约束。
        #[derive(Deserialize)]
        struct Helper<T> {
            ok: bool,
            value: Option<T>,
            error: Option<RpcError>,
        }

        let helper = Helper::<T>::deserialize(deserializer)?;
        if helper.ok {
            Ok(RpcResult::Ok {
                value: helper.value,
            })
        } else {
            let error = helper
                .error
                .ok_or_else(|| de::Error::missing_field("error"))?;
            Ok(RpcResult::Failed { error })
        }
    }
}

impl<T: Serialize> Serialize for RpcResult<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;

        match self {
            RpcResult::Ok { value } => {
                let len = if value.is_some() { 2 } else { 1 };
                let mut state = serializer.serialize_struct("RpcResult", len)?;
                state.serialize_field("ok", &true)?;
                if let Some(value) = value {
                    state.serialize_field("value", value)?;
                }
                state.end()
            }
            RpcResult::Failed { error } => {
                let mut state = serializer.serialize_struct("RpcResult", 2)?;
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", error)?;
                state.end()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_envelope_matches_wire_shape() {
        let request = ClientRequest::new(
            "rpc-1",
            "session/prompt",
            Args::new(json!({ "request": { "sessionId": "s1" } })),
        );
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(
            value,
            json!({
                "type": "client-request",
                "rpcId": "rpc-1",
                "method": "session/prompt",
                "payload": { "args": { "request": { "sessionId": "s1" } } }
            })
        );
    }

    #[test]
    fn response_with_value() {
        let raw = json!({
            "type": "server-response",
            "rpcId": "rpc-1",
            "result": { "ok": true, "value": { "sessionId": "s1", "agentPreset": "standard" } }
        });
        let response: ServerResponse<serde_json::Value> = serde_json::from_value(raw).unwrap();
        assert_eq!(response.kind, TYPE_SERVER_RESPONSE);
        match response.result {
            RpcResult::Ok { value } => {
                assert_eq!(value.unwrap()["sessionId"], json!("s1"));
            }
            other => panic!("期望 Ok，实际 {other:?}"),
        }
    }

    #[test]
    fn response_void_omits_value_field() {
        let raw = json!({
            "type": "server-response",
            "rpcId": "rpc-2",
            "result": { "ok": true }
        });
        let response: ServerResponse<()> = serde_json::from_value(raw).unwrap();
        assert!(matches!(response.result, RpcResult::Ok { value: None }));
    }

    #[test]
    fn response_business_error_is_not_transport_error() {
        let raw = json!({
            "type": "server-response",
            "rpcId": "rpc-3",
            "result": {
                "ok": false,
                "error": { "code": "session/not-found", "message": "no such session",
                           "details": { "sessionId": "s1" } }
            }
        });
        let response: ServerResponse<serde_json::Value> = serde_json::from_value(raw).unwrap();
        let error = response.result.error().expect("应为业务错误");
        assert_eq!(error.code, "session/not-found");
        assert_eq!(error.details["sessionId"], json!("s1"));
    }

    #[test]
    fn result_round_trips() {
        let ok: RpcResult<serde_json::Value> = RpcResult::Ok {
            value: Some(json!({"accepted": true})),
        };
        assert_eq!(
            serde_json::to_value(&ok).unwrap(),
            json!({"ok": true, "value": {"accepted": true}})
        );

        let failed: RpcResult<serde_json::Value> = RpcResult::Failed {
            error: RpcError {
                code: "gateway/protocol".into(),
                message: "bad frame".into(),
                details: json!({}),
            },
        };
        let value = serde_json::to_value(&failed).unwrap();
        assert_eq!(value["ok"], json!(false));
        assert_eq!(value["error"]["code"], json!("gateway/protocol"));
    }
}
