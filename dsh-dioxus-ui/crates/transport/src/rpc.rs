//! 一元 RPC 客户端。
//!
//! 线上形状（见 `dsh_proto::envelope`）：
//! ```text
//! POST /api/<namespace>/<method>
//! { "type":"client-request", "rpcId":"…", "method":"<ns>/<m>", "payload": { "args": { … } } }
//! → { "type":"server-response", "rpcId":"…", "result": { "ok": true, "value": … } }
//! ```
//!
//! 注意：业务失败同样是 **HTTP 200**，必须解析信封而不是只看状态码。

use std::time::Duration;

use dsh_proto::endpoint::api_path;
use dsh_proto::{Args, ClientRequest, RpcResult, ServerResponse};
use reqwest::Url;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::credential::Credential;
use crate::error::TransportError;

/// 一元调用客户端。内部复用一个 `reqwest::Client`（连接池在 loopback 下开销极小）。
#[derive(Clone)]
pub struct RpcClient {
    http: reqwest::Client,
    base: Url,
    cookie: Option<String>,
}

impl RpcClient {
    /// `timeout` 是一元调用的上限；长任务应走流，而不是一元。
    pub fn new(
        base: Url,
        credential: Option<&Credential>,
        timeout: Duration,
    ) -> Result<Self, TransportError> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            http,
            base,
            cookie: credential.map(|c| c.header_value().to_string()),
        })
    }

    pub fn base_url(&self) -> &Url {
        &self.base
    }

    /// 调用一个端点，返回 `Ok(None)` 表示协议上的 void 结果。
    pub async fn call<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        args: Value,
    ) -> Result<Option<T>, TransportError> {
        let url = self
            .base
            .join(api_path(endpoint).trim_start_matches('/'))
            .map_err(|err| TransportError::Protocol(format!("拼接 URL 失败: {err}")))?;

        let request =
            ClientRequest::new(uuid::Uuid::new_v4().to_string(), endpoint, Args::new(args));

        let mut builder = self.http.post(url).json(&request);
        if let Some(cookie) = &self.cookie {
            builder = builder.header(reqwest::header::COOKIE, cookie);
        }

        let response = builder.send().await?;
        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            let snippet: String = body.chars().take(200).collect();
            return Err(TransportError::Protocol(format!(
                "{endpoint} 返回 HTTP {status}: {snippet}"
            )));
        }

        let response: ServerResponse<T> = serde_json::from_str(&body).map_err(|err| {
            TransportError::Protocol(format!("{endpoint} 响应不是合法信封: {err}"))
        })?;

        match response.result {
            RpcResult::Ok { value } => Ok(value),
            RpcResult::Failed { error } => Err(TransportError::Rpc(error)),
        }
    }

    /// 不关心返回类型的调用。
    pub async fn call_value(
        &self,
        endpoint: &str,
        args: Value,
    ) -> Result<Option<Value>, TransportError> {
        self.call::<Value>(endpoint, args).await
    }

    /// 原始字节上传（`session/uploadFileBinary`）。
    ///
    /// 这条路由不使用 `ClientRequest` 信封，响应是扁平的
    /// `{ "ok": true, "value": … }` / `{ "ok": false, "error": … }`。
    /// `path` 允许携带 query（`sessionId` / `mediaType` / `name`）。
    pub async fn post_raw(
        &self,
        path: &str,
        bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<Value, TransportError> {
        let url = self
            .base
            .join(path.trim_start_matches('/'))
            .map_err(|err| TransportError::Protocol(format!("拼接上传 URL 失败: {err}")))?;
        self.post_raw_url(url, bytes, content_type).await
    }

    /// 原始字节上传（调用方已拼好完整 URL）。
    pub async fn post_raw_url(
        &self,
        url: reqwest::Url,
        bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<Value, TransportError> {
        let mut builder = self
            .http
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(bytes);
        if let Some(cookie) = &self.cookie {
            builder = builder.header(reqwest::header::COOKIE, cookie);
        }

        let response = builder.send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            let snippet: String = body.chars().take(200).collect();
            return Err(TransportError::Protocol(format!(
                "上传返回 HTTP {status}: {snippet}"
            )));
        }

        #[derive(serde::Deserialize)]
        struct RawResult {
            ok: bool,
            #[serde(default)]
            value: Option<Value>,
            #[serde(default)]
            error: Option<dsh_proto::RpcError>,
        }

        let result: RawResult = serde_json::from_str(&body)
            .map_err(|err| TransportError::Protocol(format!("上传响应不是合法结果: {err}")))?;
        if result.ok {
            result
                .value
                .ok_or_else(|| TransportError::Protocol("上传成功但缺少 value".into()))
        } else {
            Err(TransportError::Rpc(result.error.ok_or_else(|| {
                dsh_proto::RpcError {
                    code: "gateway/internal".into(),
                    message: "上传失败且没有 error".into(),
                    details: Value::Null,
                }
            })?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 只验证 URL 拼接与信封构造，不发起网络请求。
    #[test]
    fn builds_api_url_from_base() {
        let base = Url::parse("http://127.0.0.1:3080/").unwrap();
        let url = base
            .join(api_path("session/prompt").trim_start_matches('/'))
            .unwrap();
        assert_eq!(url.as_str(), "http://127.0.0.1:3080/api/session/prompt");
    }

    #[test]
    fn request_envelope_is_well_formed() {
        let request = ClientRequest::new(
            "rpc-1",
            "session/list",
            Args::new(json!({ "_request": {} })),
        );
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["type"], json!("client-request"));
        assert_eq!(value["method"], json!("session/list"));
        assert_eq!(value["payload"]["args"]["_request"], json!({}));
    }
}
