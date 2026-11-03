//! 端到端的帧级测试：用接近真实抓包形态的 JSONL 驱动解析。
//!
//! 这些 fixture 是从上游类型定义手工构造的**契约样本**：一旦上游改协议，
//! 这里应当先失败，而不是等到联调时才发现。

use dsh_proto::session::{SessionFollowFrame, SurfaceOp};
use dsh_proto::stream::{MuxClientMsg, MuxServerMsg};

/// 打开会话后的前几帧：snapshot → 流式 start/chunk/end → durable 事件。
const FOLLOW_JSONL: &str = r#"
{"type":"snapshot","header":{"version":1,"id":"s1","createdAt":1700000000000,"cwd":"/home/me/proj","isSeeded":false},"cursor":2,"records":[{"type":"event","event":{"type":"turn/start","seq":1,"time":1,"data":{"turn":1}}},{"type":"event","event":{"type":"user/message","seq":2,"time":2,"data":{"text":"hi"},"surfaceOp":"append"}}],"hasMore":false,"projections":{"asOfSeq":2,"values":{}}}
{"type":"assistant-stream","frame":{"type":"start","attemptId":"a1","revision":1,"startedAfterSeq":2,"turn":1,"step":1}}
{"type":"assistant-stream","frame":{"type":"chunk","attemptId":"a1","revision":1,"index":0,"time":3,"chunk":{"type":"text","text":"he"}}}
{"type":"assistant-stream","frame":{"type":"end","attemptId":"a1","revision":1,"index":1,"outcome":{"kind":"committed","eventType":"assistant/message","seq":3}}}
{"type":"event","event":{"type":"assistant/message","seq":3,"time":4,"data":{"text":"hello"},"surfaceOp":"append"}}
{"type":"event","event":{"type":"compaction/summary","seq":4,"time":5,"data":{"summary":"…"}}}
{"type":"event","event":{"type":"plugin/unknown","seq":5,"time":6,"data":{},"ignorable":true}}
"#;

#[test]
fn parses_a_realistic_follow_sequence() {
    let frames: Vec<SessionFollowFrame> = FOLLOW_JSONL
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("帧应当可解析"))
        .collect();

    assert_eq!(frames.len(), 7);

    // 1. snapshot 是整窗替换的锚点
    match &frames[0] {
        SessionFollowFrame::Snapshot {
            cursor,
            records,
            has_more,
            ..
        } => {
            assert_eq!(*cursor, 2);
            assert_eq!(records.len(), 2);
            assert!(!has_more);
        }
        other => panic!("首帧必须是 snapshot，实际 {other:?}"),
    }

    // 2. 流式帧不进入 durable 记录
    assert!(matches!(
        &frames[1],
        SessionFollowFrame::AssistantStream { .. }
    ));

    // 3. durable 事件的 seq 必须单调递增
    let mut last_seq = 2;
    for frame in frames.iter().skip(4) {
        if let Some(seq) = frame.seq() {
            assert!(seq > last_seq, "seq 应当递增：{seq} <= {last_seq}");
            last_seq = seq;
        }
    }

    // 4. 未知类型可安全跳过
    let unknown = match &frames[6] {
        SessionFollowFrame::Event { event } => event,
        other => panic!("期望 event，实际 {other:?}"),
    };
    assert!(unknown.is_ignorable());
}

#[test]
fn compaction_replace_semantics_are_visible() {
    let raw = r#"{"type":"event","event":{"type":"user/message","seq":9,"time":9,
        "data":{"text":"[summary]"},"surfaceOp":{"op":"replace","startSeq":2,"endSeq":8}}}"#;
    let frame: SessionFollowFrame = serde_json::from_str(raw).unwrap();
    let SessionFollowFrame::Event { event } = frame else {
        panic!("期望 event 帧");
    };
    let op = event.surface_op.expect("应带 surfaceOp");
    assert!(matches!(op, SurfaceOp::Replace { .. }));
    assert_eq!(op.replacement(), Some((2, 8)));
}

#[test]
fn append_string_is_not_a_replacement() {
    let op: SurfaceOp = serde_json::from_str(r#""append""#).unwrap();
    assert_eq!(op.replacement(), None);
}

#[test]
fn mux_frames_round_trip() {
    let open = MuxClientMsg::open(
        "stream-1",
        "session/follow",
        serde_json::json!({ "args": { "request": { "activate": true } } }),
    );
    let text = serde_json::to_string(&open).unwrap();
    assert_eq!(serde_json::from_str::<MuxClientMsg>(&text).unwrap(), open);

    let item = MuxServerMsg::Item {
        stream_id: "stream-1".into(),
        value: Some(serde_json::json!({"ok": true})),
    };
    let text = serde_json::to_string(&item).unwrap();
    assert_eq!(serde_json::from_str::<MuxServerMsg>(&text).unwrap(), item);
}

#[test]
fn every_mux_frame_carries_stream_id() {
    let frames = [
        r#"{"type":"item","streamId":"a"}"#,
        r#"{"type":"end","streamId":"a"}"#,
    ];
    for raw in frames {
        let msg: MuxServerMsg = serde_json::from_str(raw).unwrap();
        assert_eq!(msg.stream_id(), "a");
    }
}
