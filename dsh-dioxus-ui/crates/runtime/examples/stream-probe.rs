//! 打印一次真实 turn 的流式帧，定位「只显示流式中、看不到思考/正文」的问题。
//!
//! ```bash
//! DSH_BIN=./scripts/dev-host.sh cargo run -p dsh-runtime --example stream-probe
//! ```

use std::time::Duration;

use dsh_proto::session::{AssistantStreamFrame, SessionFollowFrame};
use dsh_runtime::DshClient;
use dsh_state::SessionView;
use dsh_transport::StreamEvent;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = DshClient::launch().await?;
    let created = client.create_session(None).await?;
    let session = created.session_id.as_str().to_string();
    println!("session = {session}");

    let mut follow = client.follow(session.clone())?;
    let mut view = SessionView::new();
    client
        .pump_into(&mut follow, &mut view, 1, Duration::from_secs(10))
        .await;
    println!("snapshot cursor = {}", view.cursor);

    let prompt =
        std::env::var("DSH_PROBE_PROMPT").unwrap_or_else(|_| "只回复两个字：pong".to_string());
    client.prompt(session.clone(), prompt).await?;
    println!("prompt accepted");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
    while tokio::time::Instant::now() < deadline {
        let next = tokio::time::timeout(Duration::from_secs(10), follow.next()).await;
        match next {
            Ok(Some(StreamEvent::Frame(value))) => {
                let frame: SessionFollowFrame = serde_json::from_value(value)?;
                describe(&frame);
                view.apply(frame);
                if let Some(live) = &view.live {
                    println!(
                        "    live reasoning={} text={} idx={}",
                        live.reasoning.chars().count(),
                        live.text.chars().count(),
                        live.next_index
                    );
                }
                if view.live.is_none() && view.cursor > 0 {
                    let last = view.entries.last();
                    if last.is_some_and(|event| event.kind == "turn/end") {
                        println!("turn ended; entries={}", view.len());
                        break;
                    }
                }
            }
            Ok(Some(StreamEvent::Error(err))) => {
                println!("stream error: {err}");
                break;
            }
            Ok(Some(StreamEvent::End)) | Ok(None) => {
                println!("stream ended");
                break;
            }
            Err(_) => {
                println!("（10s 无帧）");
                break;
            }
        }
    }

    println!(
        "final: entries={} cursor={} live={}",
        view.len(),
        view.cursor,
        view.live.is_some()
    );
    follow.cancel();
    client.shutdown().await;
    Ok(())
}

fn describe(frame: &SessionFollowFrame) {
    match frame {
        SessionFollowFrame::Snapshot {
            cursor, records, ..
        } => {
            println!("snapshot cursor={cursor} records={}", records.len());
        }
        SessionFollowFrame::Event { event } => {
            let text = event
                .data
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(|content| content.as_array())
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|part| {
                            Some((
                                part.get("type")?.as_str()?,
                                part.get("text").and_then(Value::as_str).unwrap_or(""),
                            ))
                        })
                        .map(|(kind, text)| format!("{kind}:{}", text.chars().count()))
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default();
            println!("event {} seq={} [{text}]", event.kind, event.seq);
        }
        SessionFollowFrame::AssistantStream { frame } => match frame {
            AssistantStreamFrame::Start {
                attempt_id,
                revision,
                turn,
                step,
                ..
            } => {
                println!("stream start attempt={attempt_id} rev={revision} turn={turn} step={step}")
            }
            AssistantStreamFrame::Chunk {
                index,
                chunk,
                revision,
                ..
            } => merge_chunk(index, revision, chunk),
            AssistantStreamFrame::End { outcome, .. } => println!("stream end: {outcome:?}"),
        },
    }
}

fn merge_chunk(index: &u64, revision: &u64, chunk: &Value) {
    let kind = chunk.get("type").and_then(Value::as_str).unwrap_or("?");
    let text = chunk.get("text").and_then(Value::as_str).unwrap_or("");
    println!(
        "chunk #{index} rev={revision} {kind} text={:?}",
        text.chars().take(80).collect::<String>()
    );
}
