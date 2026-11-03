//! 与 deepseek-harness Host 的传输层。
//!
//! 分层（见 `doc/ARCH.md` §8）：
//!
//! ```text
//! HostSupervisor  → 拉起 `dsh web --no-open --port 0`，解析 URL + token
//! Credential      → GET /connect（拿 API 客户端 cookie）
//! RpcClient       → POST /api/<ns>/<method>（一元）
//! MuxConnection   → WS /api/remote.mux（逻辑流多路复用）
//! Backoff         → 重连退避
//! ```
//!
//! 为什么网络放在原生侧（而不是 WebView 里）：原生请求不带 `Origin`，
//! 不触发 Host 的 Origin 同源栅栏，也就不需要内嵌代理或 cookie 同源。
//!
//! ## 典型用法
//!
//! ```no_run
//! # async fn demo() -> Result<(), dsh_transport::TransportError> {
//! use dsh_transport::{Credential, HostConfig, HostSupervisor, MuxConnection, RpcClient};
//! use std::time::Duration;
//!
//! let host = HostSupervisor::spawn(HostConfig::default()).await?;
//! let http = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build()?;
//! let credential = Credential::acquire(&http, host.base_url(), host.token()).await?;
//!
//! let rpc = RpcClient::new(host.base_url().clone(), Some(&credential), Duration::from_secs(30))?;
//! let mux = MuxConnection::connect(host.base_url(), Some(&credential)).await?;
//! # let _ = (rpc, mux, host);
//! # Ok(())
//! # }
//! ```

pub mod backoff;
pub mod credential;
pub mod error;
pub mod host;
pub mod mux;
pub mod rpc;

pub use backoff::Backoff;
pub use credential::{Credential, acquire, acquire_api, acquire_browser};
pub use error::TransportError;
pub use host::{HostConfig, HostSupervisor, LaunchInfo, parse_launch_line};
pub use mux::{MuxConnection, StreamEvent, StreamHandle};
pub use rpc::RpcClient;

impl Credential {
    /// 便捷方法：`GET /connect` 取凭据，失败回退浏览器 cookie 路径。
    pub async fn acquire(
        http: &reqwest::Client,
        base: &reqwest::Url,
        token: &str,
    ) -> Result<Self, TransportError> {
        acquire(http, base, token).await
    }
}
