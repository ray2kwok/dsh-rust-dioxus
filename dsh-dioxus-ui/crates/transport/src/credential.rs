//! 凭据获取：launch token → cookie。
//!
//! 上游事实（`packages/client/connection/src/browser-auth.ts`、`connect-protocol.ts`）：
//!
//! - 浏览器路径：`GET /?token=…` → `303` + `Set-Cookie: dsh-auth-<hash>=…`（v1）
//! - API 客户端路径：`GET /connect?token=…&interactions=answer` → `302` + 独立 cookie（v2，含 PeerId）
//!
//! 我们只有一条 WebSocket，正好符合 v2 的使用约束（同一 cookie 只能拥有一个 WS）。
//! 原生请求不带 `Origin`，因此不会触发 Host 的 Origin 同源栅栏。

use reqwest::Url;
use reqwest::header::SET_COOKIE;

use crate::error::TransportError;

/// API 客户端握手路径。
pub const CONNECT_PATH: &str = "connect";
/// 交互模式：`answer` 才能收到并回答 waterfall（审批）。
pub const INTERACTIONS_ANSWER: &str = "answer";

/// 后续请求要带的 cookie（形如 `dsh-auth-xxx=v1.xxx`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credential {
    cookie: String,
}

impl Credential {
    pub fn new(cookie: impl Into<String>) -> Self {
        Self {
            cookie: cookie.into(),
        }
    }

    pub fn header_value(&self) -> &str {
        &self.cookie
    }
}

/// 从 `Set-Cookie` 头里取出 `name=value` 部分（丢弃属性）。
///
/// 多个 cookie 用 `; ` 拼接——Host 可能同时下发多个 Set-Cookie。
pub fn cookie_from_set_cookie<'a>(values: impl Iterator<Item = &'a str>) -> Option<String> {
    let pairs: Vec<&str> = values
        .filter_map(|raw| {
            let pair = raw.split(';').next()?.trim();
            if pair.is_empty() || !pair.contains('=') {
                None
            } else {
                Some(pair)
            }
        })
        .collect();

    if pairs.is_empty() {
        None
    } else {
        Some(pairs.join("; "))
    }
}

/// 用 `/connect` 换取 API 客户端 cookie。
pub async fn acquire_api(
    http: &reqwest::Client,
    base: &Url,
    token: &str,
) -> Result<Credential, TransportError> {
    let url = build_connect_url(base, token)?;

    // 关闭重定向跟随：302 的 Location 指向 WS 路由，我们要的是响应头里的 cookie
    let response = http.get(url).send().await?;
    let status = response.status();
    if !(status.is_redirection() || status.is_success()) {
        return Err(TransportError::Protocol(format!(
            "/connect 返回 {status}（token 可能失效）"
        )));
    }

    let cookie = cookie_from_set_cookie(
        response
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok()),
    )
    .ok_or_else(|| TransportError::Protocol("/connect 未返回 Set-Cookie".into()))?;

    Ok(Credential::new(cookie))
}

/// 浏览器路径兜底：`GET /?token=…` 取 v1 cookie。
///
/// 仅当 `/connect` 的行为与预期不符时使用（见 `doc/ARCH.md` §4.2 的兜底表）。
pub async fn acquire_browser(
    http: &reqwest::Client,
    base: &Url,
    token: &str,
) -> Result<Credential, TransportError> {
    let mut url = base.clone();
    url.query_pairs_mut().append_pair("token", token);

    let response = http.get(url).send().await?;
    let status = response.status();
    if !(status.is_redirection() || status.is_success()) {
        return Err(TransportError::Protocol(format!(
            "/?token=… 返回 {status}（token 可能失效）"
        )));
    }

    let cookie = cookie_from_set_cookie(
        response
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok()),
    )
    .ok_or_else(|| TransportError::Protocol("根路径未返回 Set-Cookie".into()))?;

    Ok(Credential::new(cookie))
}

/// 先走 `/connect`，失败再退回浏览器路径。
pub async fn acquire(
    http: &reqwest::Client,
    base: &Url,
    token: &str,
) -> Result<Credential, TransportError> {
    match acquire_api(http, base, token).await {
        Ok(credential) => Ok(credential),
        Err(err) => {
            tracing::warn!(%err, "`/connect` 取得凭据失败，回退到浏览器 cookie 路径");
            acquire_browser(http, base, token).await
        }
    }
}

fn build_connect_url(base: &Url, token: &str) -> Result<Url, TransportError> {
    let mut url = base
        .join(CONNECT_PATH)
        .map_err(|err| TransportError::Protocol(format!("拼接 /connect 失败: {err}")))?;
    url.query_pairs_mut()
        .append_pair("token", token)
        .append_pair("interactions", INTERACTIONS_ANSWER);
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_name_value_pairs() {
        let raw = [
            "dsh-auth-abc=v1.payload.sig; Path=/; HttpOnly; SameSite=Strict",
            "other=1; Path=/",
        ];
        let cookie = cookie_from_set_cookie(raw.iter().copied()).expect("应提取成功");
        assert_eq!(cookie, "dsh-auth-abc=v1.payload.sig; other=1");
    }

    #[test]
    fn ignores_malformed_entries() {
        assert_eq!(
            cookie_from_set_cookie(["", "novalue"].into_iter()),
            None,
            "空值或缺 '=' 的项应被丢弃"
        );
    }

    #[test]
    fn connect_url_carries_token_and_interactions() {
        let base = Url::parse("http://127.0.0.1:3080/").unwrap();
        let url = build_connect_url(&base, "tok").unwrap();
        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:3080/connect?token=tok&interactions=answer"
        );
    }
}
