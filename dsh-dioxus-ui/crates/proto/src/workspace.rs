//! 工作区（Workspace）契约。
//!
//! 上游出处：`packages/api/workspace-controller/src/types.ts`。

use serde::{Deserialize, Serialize};

/// 一个工作区条目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceView {
    pub workspace_id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub session_ids: Vec<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

/// `workspace/follow` 的连接基线。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBaseline {
    #[serde(default)]
    pub items: Vec<WorkspaceView>,
    #[serde(default)]
    pub archived_session_ids: Vec<String>,
    #[serde(default)]
    pub pinned_session_ids: Vec<String>,
}

/// `workspace/follow` 的下行帧。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WorkspaceFollowFrame {
    Baseline {
        value: WorkspaceBaseline,
    },
    Upsert {
        workspace: WorkspaceView,
    },
    Remove {
        #[serde(rename = "workspaceId")]
        workspace_id: String,
    },
    Order {
        #[serde(rename = "workspaceIds")]
        workspace_ids: Vec<String>,
    },
    Archived {
        #[serde(rename = "archivedSessionIds")]
        archived_session_ids: Vec<String>,
    },
    Pinned {
        #[serde(rename = "pinnedSessionIds")]
        pinned_session_ids: Vec<String>,
    },
    Replace {
        value: WorkspaceBaseline,
    },
}

impl WorkspaceFollowFrame {
    pub fn baseline(&self) -> Option<&WorkspaceBaseline> {
        match self {
            Self::Baseline { value } | Self::Replace { value } => Some(value),
            _ => None,
        }
    }

    pub fn archived_ids(&self) -> Option<Vec<String>> {
        match self {
            Self::Baseline { value } | Self::Replace { value } => {
                Some(value.archived_session_ids.clone())
            }
            Self::Archived {
                archived_session_ids,
            } => Some(archived_session_ids.clone()),
            _ => None,
        }
    }

    pub fn pinned_ids(&self) -> Option<Vec<String>> {
        match self {
            Self::Baseline { value } | Self::Replace { value } => {
                Some(value.pinned_session_ids.clone())
            }
            Self::Pinned { pinned_session_ids } => Some(pinned_session_ids.clone()),
            _ => None,
        }
    }
}

/// `workspace/pinSession` / `unpinSession` 的返回。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePinValue {
    #[serde(default)]
    pub pinned_session_ids: Vec<String>,
}

/// 创建/重命名返回的完整工作区。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceValue {
    pub workspace: WorkspaceView,
}

/// 创建结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCreateValue {
    pub workspace: WorkspaceView,
    #[serde(default)]
    pub created: bool,
}

/// `workspace/insertBefore` 的返回。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceOrderValue {
    #[serde(default)]
    pub workspace_ids: Vec<String>,
}
