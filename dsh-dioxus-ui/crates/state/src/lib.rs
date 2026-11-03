//! 会话视图与事件归约。
//!
//! 这一层是**纯逻辑**：输入是 [`SessionFollowFrame`]，输出是对 [`SessionView`] 的变更，
//! 不碰网络、不碰 UI，因此可以用录制帧在 `cargo test` 里完整回归。
//!
//! 三条不可动摇的语义（对应上游 `packages/api/session-controller`）：
//!
//! 1. `snapshot` 是**整窗替换**（重连后必须覆盖本地窗口，而不是追加）；
//! 2. durable 事件按 `seq` 严格递增，重复/过期事件应丢弃；
//! 3. `surfaceOp: { op: "replace", startSeq, endSeq }` 表示「用本事件替换区间」，
//!    compaction 依赖这一条，不能简单 append。

use dsh_proto::session::{
    AssistantStreamFrame, SessionFollowFrame, SessionWireEvent, SessionWireHeader, SurfaceOp,
};
use serde_json::Value;

/// 归约结果，便于上层决定是否需要滚动/重排。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    /// 整窗替换（快照）。
    Snapshot,
    /// 追加一条事件。
    Append { seq: u64 },
    /// 用一条事件替换了区间 `[start, end]`。
    Replace { seq: u64, start: u64, end: u64 },
    /// 流式帧（不影响 durable 窗口）。
    Stream,
    /// 过期或重复，已丢弃。
    Ignored,
}

/// 正在进行中的 assistant 流式尝试。
#[derive(Debug, Clone, PartialEq)]
pub struct LiveAttempt {
    pub attempt_id: String,
    pub revision: u64,
    pub turn: u32,
    pub step: u32,
    /// 期望的下一帧稠密序号。
    pub next_index: u64,
    /// 已累积的文本（从 `type == "text"` 的 chunk 里抽取）。
    pub text: String,
    /// 原始 chunk，便于调试与兜底重建。
    pub chunks: Vec<Value>,
}

/// 从一个 chunk 里抽取可显示文本；非文本 chunk 忽略。
fn append_text(target: &mut String, chunk: &Value) {
    if chunk.get("type").and_then(Value::as_str) == Some("text")
        && let Some(text) = chunk.get("text").and_then(Value::as_str)
    {
        target.push_str(text);
    }
}

/// 单个会话的视图。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionView {
    pub header: Option<SessionWireHeader>,
    /// 已归约的 durable 事件，按 `seq` 升序。
    pub entries: Vec<SessionWireEvent>,
    /// 已覆盖到的最大 seq。
    pub cursor: u64,
    pub has_more: bool,
    pub live: Option<LiveAttempt>,
    pub projections: Value,
}

impl SessionView {
    pub fn new() -> Self {
        Self::default()
    }

    /// 应用一帧。
    pub fn apply(&mut self, frame: SessionFollowFrame) -> Applied {
        match frame {
            SessionFollowFrame::Snapshot {
                header,
                cursor,
                records,
                has_more,
                projections,
                assistant_stream,
            } => {
                self.header = Some(header);
                self.entries = records.into_iter().map(|record| record.event).collect();
                self.cursor = cursor;
                self.has_more = has_more;
                self.projections = projections.values;
                // revision 在 baseline 上，不在 attempt 内
                self.live = assistant_stream.and_then(|baseline| {
                    let revision = baseline.revision;
                    baseline.active_attempt.map(|attempt| LiveAttempt {
                        attempt_id: attempt.attempt_id,
                        revision,
                        turn: attempt.turn,
                        step: attempt.step,
                        next_index: attempt.next_index,
                        text: attempt.stream.iter().fold(String::new(), |mut acc, chunk| {
                            append_text(&mut acc, chunk);
                            acc
                        }),
                        chunks: attempt.stream,
                    })
                });
                Applied::Snapshot
            }

            SessionFollowFrame::Event { event } => self.apply_event(event),

            SessionFollowFrame::AssistantStream { frame } => {
                self.apply_stream(frame);
                Applied::Stream
            }
        }
    }

    fn apply_event(&mut self, event: SessionWireEvent) -> Applied {
        let replacement = event.surface_op.as_ref().and_then(SurfaceOp::replacement);

        if let Some((start, end)) = replacement {
            self.entries
                .retain(|existing| existing.seq < start || existing.seq > end);
            self.entries.push(event.clone());
            self.entries.sort_by_key(|entry| entry.seq);
            self.cursor = self.cursor.max(event.seq);
            return Applied::Replace {
                seq: event.seq,
                start,
                end,
            };
        }

        // 单调性守卫：重复/过期事件丢弃（重连后由 snapshot 整窗纠正）
        if event.seq <= self.cursor && !self.entries.is_empty() {
            return Applied::Ignored;
        }

        self.cursor = self.cursor.max(event.seq);
        self.entries.push(event.clone());
        Applied::Append { seq: event.seq }
    }

    fn apply_stream(&mut self, frame: AssistantStreamFrame) {
        match frame {
            AssistantStreamFrame::Start {
                attempt_id,
                revision,
                turn,
                step,
                ..
            } => {
                self.live = Some(LiveAttempt {
                    attempt_id,
                    revision,
                    turn,
                    step,
                    next_index: 0,
                    text: String::new(),
                    chunks: Vec::new(),
                });
            }
            AssistantStreamFrame::Chunk {
                attempt_id,
                revision,
                index,
                chunk,
                ..
            } => {
                let Some(live) = self.live.as_mut() else {
                    return;
                };
                if live.attempt_id != attempt_id || live.revision != revision {
                    // 属于别的 attempt（例如重连前的残留），丢弃
                    return;
                }
                append_text(&mut live.text, &chunk);
                live.chunks.push(chunk);
                live.next_index = index.saturating_add(1);
            }
            AssistantStreamFrame::End { .. } => {
                // 提交后的事件会以 durable 形式随后到达，这里只清掉临时态
                self.live = None;
            }
        }
    }

    /// 向前插入更早的历史页（`session/page` 的结果）。
    ///
    /// 与 `snapshot` 的「整窗替换」不同：这里保留现有窗口，只把更早的记录
    /// 拼到前面，并更新 `has_more`。重复 seq 会被丢弃。
    pub fn prepend(&mut self, records: Vec<SessionWireEvent>, has_more: bool) {
        let mut older = records;
        older.retain(|event| {
            !self
                .entries
                .iter()
                .any(|existing| existing.seq == event.seq)
        });
        if older.is_empty() {
            self.has_more = has_more;
            return;
        }
        older.extend(std::mem::take(&mut self.entries));
        older.sort_by_key(|event| event.seq);
        self.entries = older;
        self.has_more = has_more;
    }

    /// 读一条 projection（`snapshot.projections.values[key]`）。
    ///
    /// 上游的 projection 是 `SessionProjectionBaseline.values` 里的扁平 map，
    /// 键由各插件注册（`title` / `inbox` / `contextPressure` / `sessionStats` …）。
    pub fn projection(&self, key: &str) -> Option<&Value> {
        self.projections.get(key)
    }

    /// 已归约的事件数。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 按 `type` 前缀筛选事件（如 `tool/`）。
    pub fn events_of<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a SessionWireEvent> {
        self.entries.iter().filter(move |event| event.kind == kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn frame(raw: Value) -> SessionFollowFrame {
        serde_json::from_value(raw).expect("帧应当可解析")
    }

    fn snapshot(records: Value, cursor: u64) -> SessionFollowFrame {
        frame(json!({
            "type": "snapshot",
            "header": { "version": 1, "id": "s1", "createdAt": 1, "isSeeded": false },
            "cursor": cursor,
            "records": records,
            "hasMore": false,
            "projections": { "asOfSeq": cursor, "values": {} }
        }))
    }

    fn user_message(seq: u64) -> Value {
        json!({
            "type": "event",
            "event": { "type": "user/message", "seq": seq, "time": seq, "data": { "text": "hi" } }
        })
    }

    #[test]
    fn snapshot_replaces_window() {
        let mut view = SessionView::new();
        let applied = view.apply(snapshot(json!([user_message(1), user_message(2)]), 2));
        assert_eq!(applied, Applied::Snapshot);
        assert_eq!(view.len(), 2);
        assert_eq!(view.cursor, 2);
        assert!(view.header.is_some());
    }

    #[test]
    fn appends_in_order_and_drops_duplicates() {
        let mut view = SessionView::new();
        view.apply(snapshot(json!([user_message(1)]), 1));

        assert_eq!(
            view.apply(frame(json!({
                "type": "event",
                "event": { "type": "assistant/message", "seq": 2, "time": 2, "data": {} }
            }))),
            Applied::Append { seq: 2 }
        );
        assert_eq!(view.len(), 2);

        // 重复 seq 应被丢弃，避免重连后重复消息
        assert_eq!(
            view.apply(frame(json!({
                "type": "event",
                "event": { "type": "assistant/message", "seq": 2, "time": 2, "data": {} }
            }))),
            Applied::Ignored
        );
        assert_eq!(view.len(), 2);
    }

    #[test]
    fn surface_replace_swaps_range() {
        let mut view = SessionView::new();
        view.apply(snapshot(
            json!([
                user_message(1),
                user_message(2),
                user_message(3),
                user_message(4)
            ]),
            4,
        ));
        assert_eq!(view.len(), 4);

        let applied = view.apply(frame(json!({
            "type": "event",
            "event": {
                "type": "user/message", "seq": 5, "time": 5,
                "data": { "text": "[summary]" },
                "surfaceOp": { "op": "replace", "startSeq": 2, "endSeq": 4 }
            }
        })));

        assert_eq!(
            applied,
            Applied::Replace {
                seq: 5,
                start: 2,
                end: 4
            }
        );
        // 1 保留，2..=4 被替换为 5
        assert_eq!(view.len(), 2);
        assert_eq!(view.entries[0].seq, 1);
        assert_eq!(view.entries[1].seq, 5);
        assert_eq!(view.cursor, 5);
    }

    #[test]
    fn stream_chunks_accumulate_text_then_clear_on_end() {
        let mut view = SessionView::new();
        view.apply(snapshot(json!([]), 0));

        view.apply(frame(json!({
            "type": "assistant-stream",
            "frame": { "type": "start", "attemptId": "a1", "revision": 1,
                       "startedAfterSeq": 0, "turn": 1, "step": 1 }
        })));
        assert!(view.live.is_some());

        for (index, piece) in ["he", "llo"].iter().enumerate() {
            view.apply(frame(json!({
                "type": "assistant-stream",
                "frame": { "type": "chunk", "attemptId": "a1", "revision": 1,
                           "index": index as u64, "time": index as u64,
                           "chunk": { "type": "text", "text": piece } }
            })));
        }

        let live = view.live.as_ref().expect("应有进行中的 attempt");
        assert_eq!(live.text, "hello");
        assert_eq!(live.next_index, 2);

        view.apply(frame(json!({
            "type": "assistant-stream",
            "frame": { "type": "end", "attemptId": "a1", "revision": 1, "index": 2,
                       "outcome": { "kind": "committed", "eventType": "assistant/message", "seq": 1 } }
        })));
        assert!(view.live.is_none(), "流式结束后应清理临时态");
    }

    #[test]
    fn stale_stream_frames_are_dropped() {
        let mut view = SessionView::new();
        view.apply(frame(json!({
            "type": "assistant-stream",
            "frame": { "type": "start", "attemptId": "a1", "revision": 2,
                       "startedAfterSeq": 0, "turn": 1, "step": 1 }
        })));
        view.apply(frame(json!({
            "type": "assistant-stream",
            "frame": { "type": "chunk", "attemptId": "old", "revision": 1,
                       "index": 0, "time": 0, "chunk": { "type": "text", "text": "x" } }
        })));
        assert_eq!(view.live.as_ref().unwrap().text, "");
    }

    #[test]
    fn reconnect_snapshot_overwrites_local_state() {
        let mut view = SessionView::new();
        view.apply(snapshot(json!([user_message(1)]), 1));
        view.apply(frame(json!({
            "type": "event",
            "event": { "type": "assistant/message", "seq": 2, "time": 2, "data": {} }
        })));
        assert_eq!(view.len(), 2);

        // 重连：Host 给出更完整的窗口（含 3 条），必须整体替换
        view.apply(snapshot(
            json!([user_message(1), user_message(2), user_message(3)]),
            3,
        ));
        assert_eq!(view.len(), 3);
        assert_eq!(view.cursor, 3);
    }
}
