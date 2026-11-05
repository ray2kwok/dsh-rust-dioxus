//! 模型提供方目录与凭据契约。
//!
//! 上游出处：
//! - `packages/llm/llm/src/types.ts`（`LlmProviderInfo` / `LlmConfigurableProvider` / `LlmNativeAuth`）
//! - `packages/credentials/credentials/src/types.ts`（`CredentialInfo`）
//! - 远程方法签名：`packages/llm/llm/lib/typert.remote-client.d.ts`、
//!   `packages/api/settings-controller/lib/typert.remote-client.d.ts`
//!
//! 关键事实：可配置 provider 的 `settingsPath` 为空表示「官方实现」，其凭据引用取自
//! 该命名空间自身的 `apiKeyEnv`；非空表示 `llm-pi-ai` 的一条 route，
//! 凭据引用取自 `nativeAuth.env[0]`。

use serde::{Deserialize, Serialize};

/// 已注册（可路由）的 provider。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmProviderInfo {
    pub id: String,
    #[serde(default)]
    pub name: String,
}

/// provider 自带的凭据发现方式。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmNativeAuth {
    #[serde(default)]
    pub env: Vec<String>,
    #[serde(default)]
    pub oauth: bool,
}

/// 设置页可配置的一条 provider 条目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigurableProvider {
    pub provider: String,
    #[serde(default)]
    pub display_name: String,
    pub settings_ns: String,
    #[serde(default)]
    pub settings_path: Vec<String>,
    /// `true` 表示该 route 由用户在设置里声明（而非 pi-ai 内置目录）。
    #[serde(default)]
    pub declared: bool,
    #[serde(default)]
    pub default_base_url: Option<String>,
    #[serde(default)]
    pub base_url_env: Vec<String>,
    #[serde(default)]
    pub native_auth: Option<LlmNativeAuth>,
    #[serde(default)]
    pub error: Option<String>,
}

/// 一条凭据引用的状态。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialInfo {
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub writable: bool,
}
