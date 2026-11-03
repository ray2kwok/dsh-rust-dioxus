//! deepseek-harness 的 wire 协议类型。
//!
//! 设计约束（见 `doc/ARCH.md` §7）：
//!
//! - **纯数据**：不做网络、不做状态，可脱离 UI 与运行时单测；
//! - **可追溯**：每个类型标注上游出处（包 + 文件），便于与 fork 对照；
//! - **严格解析**：Host 对 WS 帧做 exact-keys 校验，这里同样不做宽松兜底，
//!   避免「本地能跑、真实 Host 401/400」。
//!
//! 命名的唯一准则：**Rust 侧 `snake_case`，线上 `camelCase`（或上游原样）**。

pub mod endpoint;
pub mod envelope;
pub mod events;
pub mod session;
pub mod stream;
pub mod subagent;

pub use envelope::{Args, ClientRequest, RpcError, RpcResult, ServerResponse};
