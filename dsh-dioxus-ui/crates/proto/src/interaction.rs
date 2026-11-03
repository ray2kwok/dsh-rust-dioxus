//! 交互面契约：命令、文件引用、模型目录、权限预设、消息反馈、Agent preset。
//!
//! 上游出处：
//! - `packages/interaction/commands/src/types.ts`
//! - `packages/context/file-reference/src/types.ts`
//! - `packages/api/session-controller/src/types.ts`（模型目录）
//! - `packages/interaction/permission-presets/src/types.ts`
//! - `packages/feedback/message-feedback/src/types.ts`
//! - `packages/preset/agent-preset-registry/src/types.ts`

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 一个斜杠命令。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandDescriptor {
    #[serde(default)]
    pub definition_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub input: Option<CommandInputDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandInputDescriptor {
    #[serde(default)]
    pub hint: String,
    #[serde(default)]
    pub attachments: bool,
}

/// 命令执行结果（`kind` 为 `success` / `error`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CommandResult {
    Success {
        #[serde(default)]
        text: Option<String>,
        #[serde(default, rename = "sourceEventSeq")]
        source_event_seq: Option<u64>,
    },
    Error {
        #[serde(default)]
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandExecution {
    pub command_id: String,
    pub result: CommandResult,
}

/// `@` 补全候选。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReferenceCandidate {
    pub path: String,
    pub kind: String,
}

/// 模型选择。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSelection {
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogModel {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub reasoning: Option<ModelReasoning>,
    #[serde(default)]
    pub badge: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelReasoning {
    #[serde(default)]
    pub efforts: Vec<Value>,
    #[serde(default)]
    pub default_effort: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProviderGroup {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub models: Vec<ModelCatalogModel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogFailure {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalog {
    pub default: ModelSelection,
    #[serde(default)]
    pub routable_providers: Vec<String>,
    #[serde(default)]
    pub groups: Vec<ModelProviderGroup>,
    #[serde(default)]
    pub failures: Vec<ModelCatalogFailure>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectModelValue {
    pub selected: ModelSelection,
}

/// 权限预设。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetOption {
    pub value: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionCatalog {
    #[serde(default)]
    pub options: Vec<PresetOption>,
    #[serde(default)]
    pub default_options: Vec<PresetOption>,
    #[serde(default)]
    pub default_preset: String,
}

/// `messageFeedback/put` 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageFeedbackPutRequest {
    pub session_id: String,
    pub message_id: String,
    pub rating: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// `null` 表示要求当前没有反馈；否则传上一次版本。
    pub if_version: Value,
}

/// Agent preset 目录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPresetRoster {
    #[serde(default)]
    pub presets: Vec<AgentPresetRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPresetRow {
    pub id: String,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub broken: Option<String>,
}

/// 文件上传返回（`receiptId` + `image` / `file`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadValue {
    pub receipt_id: String,
    #[serde(default)]
    pub image: Option<Value>,
    #[serde(default)]
    pub file: Option<Value>,
}
