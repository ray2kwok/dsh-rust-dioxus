//! 设置（Settings）契约。
//!
//! 上游出处：
//! - `packages/settings/settings/src/types.ts`（`SettingsDescribeValue` / `SettingsNamespaceView`）
//! - `packages/api/settings-controller/src/index.ts`（`describe` / `update` / `mutate`）
//!
//! `update` 的线上 args 是扁平三元组：
//! `{ "ns": "ui-theme", "patch": {…}, "expectedRevision": 3 }`。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 一个设置命名空间的脱敏视图。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsNamespaceView {
    pub ns: String,
    #[serde(default)]
    pub auto_generate: bool,
    #[serde(default)]
    pub schema: Value,
    #[serde(default)]
    pub value: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<Value>,
    #[serde(default)]
    pub applies: String,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub secrets: Value,
}

impl SettingsNamespaceView {
    /// 读取一个字段（不存在时返回 `None`）。
    pub fn field(&self, key: &str) -> Option<&Value> {
        self.value.get(key)
    }

    pub fn bool_field(&self, key: &str) -> Option<bool> {
        self.field(key).and_then(Value::as_bool)
    }

    pub fn u64_field(&self, key: &str) -> Option<u64> {
        self.field(key).and_then(Value::as_u64)
    }

    pub fn str_field(&self, key: &str) -> Option<&str> {
        self.field(key).and_then(Value::as_str)
    }
}

/// `settings/describe` 的返回。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDescribeValue {
    #[serde(default)]
    pub writable: bool,
    #[serde(default)]
    pub has_document: bool,
    #[serde(default)]
    pub namespaces: Vec<SettingsNamespaceView>,
}

impl SettingsDescribeValue {
    pub fn namespace(&self, ns: &str) -> Option<&SettingsNamespaceView> {
        self.namespaces.iter().find(|item| item.ns == ns)
    }
}

/// 主题与排版命名空间（`ui-theme`）。
pub const THEME_NS: &str = "ui-theme";
/// 语言命名空间。
pub const LOCALE_NS: &str = "locale";
/// 子 Agent 委派上限命名空间。
pub const SUBAGENT_NS: &str = "subagent";
/// 子 Agent 模型选择命名空间。
pub const SUBAGENT_MODEL_NS: &str = "subagent-model-selection-settings";
