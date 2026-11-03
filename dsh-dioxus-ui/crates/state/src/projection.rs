//! 全 Host 的 projection 缓存。
//!
//! 上游 `packages/api/session-controller/src/client/sessions/service.ts` 把
//! `session/control` 的 `baseline` 与 `projection` 增量合并进一个
//! `projectionsBySession` map；子 Agent 目录、耗时与用量都从这里读。
//!
//! 归约规则：
//! - `baseline` 是**整窗替换**（重连后旧值必须丢弃）；
//! - `projection` 只更新 `(sessionId, key)`；
//! - `session/follow` 的 snapshot 与 `session/projections` 的一元结果可作为
//!   单会话的种子，覆盖同会话的旧值。

use std::collections::{HashMap, HashSet};

use dsh_proto::session::{SessionControlFrame, SessionProjectionsValue};
use serde_json::Value;

/// 每个会话一份扁平 projection map（键由各插件注册）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProjectionStore {
    values: HashMap<String, Value>,
    /// `session/projections` 返回 `migration-required` 的会话。
    migration_required: HashSet<String>,
}

impl ProjectionStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 应用一帧 `session/control`。
    pub fn apply_control(&mut self, frame: &SessionControlFrame) {
        match frame {
            SessionControlFrame::Baseline { value } => {
                self.values = value
                    .projections
                    .iter()
                    .map(|(session_id, baseline)| (session_id.clone(), baseline.values.clone()))
                    .collect();
                self.migration_required.clear();
            }
            SessionControlFrame::Projection {
                session_id,
                key,
                value,
                ..
            } => {
                let entry = self
                    .values
                    .entry(session_id.as_str().to_string())
                    .or_insert_with(|| Value::Object(Default::default()));
                if !entry.is_object() {
                    *entry = Value::Object(Default::default());
                }
                if let Some(map) = entry.as_object_mut() {
                    map.insert(key.clone(), value.clone());
                }
            }
        }
    }

    /// 用 `session/follow` 的 snapshot 覆盖单个会话（更接近当前窗口）。
    pub fn seed(&mut self, session_id: &str, values: Value) {
        self.values.insert(session_id.to_string(), values);
    }

    /// 用 `session/projections` 的一元结果覆盖单个会话。
    pub fn seed_result(&mut self, session_id: &str, value: &SessionProjectionsValue) {
        self.values
            .insert(session_id.to_string(), value.values().clone());
        if matches!(value, SessionProjectionsValue::MigrationRequired { .. }) {
            self.migration_required.insert(session_id.to_string());
        } else {
            self.migration_required.remove(session_id);
        }
    }

    /// 该会话的历史是否需要迁移（只拿到了缓存值）。
    pub fn migration_required(&self, session_id: &str) -> bool {
        self.migration_required.contains(session_id)
    }

    /// 读一个会话的全部 projection。
    pub fn values_of(&self, session_id: &str) -> Option<&Value> {
        self.values.get(session_id)
    }

    /// 读一个会话的单个 projection。
    pub fn get(&self, session_id: &str, key: &str) -> Option<&Value> {
        self.values.get(session_id)?.get(key)
    }

    /// 已缓存的会话 id（诊断与批量刷新用）。
    pub fn session_ids(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn control(raw: Value) -> SessionControlFrame {
        serde_json::from_value(raw).expect("control 帧应可解析")
    }

    #[test]
    fn baseline_replaces_everything() {
        let mut store = ProjectionStore::new();
        store.seed("stale", json!({ "title": "old" }));
        store.apply_control(&control(json!({
            "type": "baseline",
            "value": { "projections": {
                "s1": { "asOfSeq": 1, "values": { "title": "parent" } }
            }}
        })));
        assert_eq!(store.len(), 1, "baseline 必须丢弃旧值");
        assert_eq!(store.get("s1", "title"), Some(&json!("parent")));
        assert!(store.get("stale", "title").is_none());
    }

    #[test]
    fn projection_updates_one_key_and_creates_session() {
        let mut store = ProjectionStore::new();
        store.apply_control(&control(json!({
            "type": "projection",
            "sessionId": "child",
            "key": "subagentTiming",
            "value": { "settledMs": 12 },
            "seq": 1
        })));
        assert_eq!(
            store.get("child", "subagentTiming"),
            Some(&json!({ "settledMs": 12 }))
        );

        store.apply_control(&control(json!({
            "type": "projection",
            "sessionId": "child",
            "key": "tokenUsage",
            "value": { "inputTokens": 3 },
            "seq": 2
        })));
        assert_eq!(
            store.get("child", "tokenUsage"),
            Some(&json!({ "inputTokens": 3 }))
        );
    }

    #[test]
    fn seed_result_overrides_session() {
        let mut store = ProjectionStore::new();
        store.seed("s1", json!({ "title": "old" }));
        let value: SessionProjectionsValue = serde_json::from_value(json!({
            "kind": "sequenced",
            "asOfSeq": 4,
            "values": { "title": "new", "subagentCatalog": [] }
        }))
        .unwrap();
        store.seed_result("s1", &value);
        assert_eq!(store.get("s1", "title"), Some(&json!("new")));
        assert!(!store.migration_required("s1"));
    }

    #[test]
    fn migration_required_is_tracked_and_cleared() {
        let mut store = ProjectionStore::new();
        let migration: SessionProjectionsValue = serde_json::from_value(json!({
            "kind": "migration-required",
            "values": { "subagentCatalog": [] }
        }))
        .unwrap();
        store.seed_result("old", &migration);
        assert!(store.migration_required("old"));

        let sequenced: SessionProjectionsValue = serde_json::from_value(json!({
            "kind": "sequenced",
            "asOfSeq": 9,
            "values": { "subagentCatalog": [] }
        }))
        .unwrap();
        store.seed_result("old", &sequenced);
        assert!(!store.migration_required("old"));
    }
}
