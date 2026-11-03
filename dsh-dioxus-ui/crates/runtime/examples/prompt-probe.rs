//! 校验 `session/prompt` 的发送链路（queue / steer 两种模式）。
//!
//! 没有配置模型时，Host 仍会接受消息并产生 turn 事件；如果模型不可用，
//! 事件流里会出现错误事件——两者都能证明「发送 → 事件回传」是通的。
//!
//! ```bash
//! DSH_BIN=./scripts/dev-host.sh cargo run -p dsh-runtime --example prompt-probe
//! ```

use std::time::Duration;

use dsh_proto::session::PromptMode;
use dsh_runtime::DshClient;
use dsh_state::SessionView;
use dsh_transport::StreamEvent;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = DshClient::launch().await?;
    let mut events = client.open_events()?;
    let client_id = client
        .await_ready(&mut events)
        .await
        .ok_or("未收到 ready")?;
    println!("clientId = {client_id}");

    let list = client.list_sessions().await?;
    let session = if let Some(first) = list.first() {
        first.session_id.as_str().to_string()
    } else {
        let cwd = std::env::current_dir()?.to_string_lossy().to_string();
        client
            .create_session(Some(cwd))
            .await?
            .session_id
            .as_str()
            .to_string()
    };
    println!("session = {session}");

    let mut follow = client.follow(session.clone())?;
    let mut view = SessionView::new();
    // 先消费快照
    client
        .pump_into(&mut follow, &mut view, 1, Duration::from_secs(10))
        .await;
    println!("快照 entries = {}", view.len());

    let mode = match std::env::var("DSH_PROMPT_MODE").as_deref() {
        Ok("steer") => PromptMode::Steer,
        _ => PromptMode::Queue,
    };
    println!("发送模式 = {mode:?}");

    match client
        .prompt_with_mode(session.clone(), "ping（自动探测消息）", mode)
        .await
    {
        Ok(()) => println!("prompt 已接受"),
        Err(err) => println!("prompt 被拒: {err}"),
    }

    println!("\n后续事件（最多 6 条 / 15s）：");
    for _ in 0..6 {
        match tokio::time::timeout(Duration::from_secs(15), follow.next()).await {
            Ok(Some(StreamEvent::Frame(value))) => {
                let kind = value
                    .get("event")
                    .and_then(|e| e.get("type"))
                    .and_then(|t| t.as_str())
                    .unwrap_or_else(|| value.get("type").and_then(|t| t.as_str()).unwrap_or("?"));
                println!("  - {kind}");
            }
            Ok(Some(StreamEvent::Error(err))) => {
                println!("  流错误: {err}");
                break;
            }
            Ok(Some(StreamEvent::End)) | Ok(None) => break,
            Err(_) => {
                println!("  （等待超时）");
                break;
            }
        }
    }

    let _ = client.cancel(session).await;
    follow.cancel();
    events.cancel();
    client.shutdown().await;
    Ok(())
}
