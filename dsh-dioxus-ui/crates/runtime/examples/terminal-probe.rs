//! 端到端验证用户终端：environment → create → attach → 输入 → 输出 → close。
//!
//! 不依赖 GUI，直接对真实 Host 跑一遍协议：
//!
//! ```bash
//! DSH_BIN=./scripts/dev-host.sh cargo run -p dsh-runtime --example terminal-probe
//! ```
//!
//! 成功判据：屏幕里出现 `probe-ok`（由我们在 shell 里 `printf` 出来）。

use std::time::Duration;

use dsh_proto::terminal::{TerminalCreateRequest, TerminalFrame, TerminalInput};
use dsh_runtime::DshClient;
use dsh_transport::StreamEvent;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = DshClient::launch().await?;

    // 不挑已有会话：别的 Host/GUI 可能正持有写句柄（`session/writer-held`）
    let session_id = client
        .create_session(None)
        .await?
        .session_id
        .as_str()
        .to_string();
    println!("session = {session_id}");

    let environment = client.terminal_environment(&session_id).await?;
    println!(
        "environment cwd={} cols<={} rows<={} scrollback={}",
        environment.cwd, environment.max_cols, environment.max_rows, environment.scrollback
    );

    let id = format!("probe-{}", std::process::id());
    let info = client
        .terminal_create(
            &session_id,
            TerminalCreateRequest {
                shell_path: None,
                id: id.clone(),
                cols: environment.max_cols.clamp(20, 120),
                rows: environment.max_rows.clamp(8, 60),
            },
        )
        .await?;
    println!(
        "created id={} state={} title={}",
        info.id, info.state, info.title
    );

    let mut stream = client.terminal_attach(&session_id, &info.id)?;
    let uplink = stream.uplink();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let mut screen = String::new();
    let mut sequence = 0;
    let mut sent = false;
    let mut success = false;

    while tokio::time::Instant::now() < deadline {
        let next = tokio::time::timeout(Duration::from_millis(500), stream.next()).await;
        match next {
            Ok(Some(StreamEvent::Frame(value))) => {
                let frame: TerminalFrame = serde_json::from_value(value)?;
                match frame {
                    TerminalFrame::Snapshot {
                        screen: snapshot,
                        info,
                        writable,
                        ..
                    } => {
                        println!("snapshot state={} writable={writable}", info.state);
                        screen = snapshot;
                        if !sent {
                            sequence += 1;
                            uplink.send_item(serde_json::to_value(TerminalInput::write(
                                sequence,
                                "printf 'probe-ok\\n'\n",
                            ))?);
                            sent = true;
                        }
                    }
                    TerminalFrame::Output { data, .. } => screen.push_str(&data),
                    TerminalFrame::State { info, writable } => {
                        println!("state={} writable={writable}", info.state);
                    }
                    TerminalFrame::Ack { sequence, refusal } => {
                        println!("ack sequence={sequence} refusal={refusal:?}");
                    }
                }
                if screen.contains("probe-ok") {
                    success = true;
                    break;
                }
            }
            Ok(Some(StreamEvent::Error(error))) => {
                return Err(format!("终端流出错: {error}").into());
            }
            Ok(Some(StreamEvent::End)) | Ok(None) => {
                println!("流已结束");
                break;
            }
            Err(_) => {}
        }
    }

    uplink.cancel();
    client.terminal_close(&session_id, &info.id).await?;
    println!("closed id={}", info.id);
    client.shutdown().await;

    if success {
        println!("OK: 终端输出里出现了 probe-ok");
        Ok(())
    } else {
        println!("--- 屏幕尾部 ---\n{}", tail(&screen, 800));
        Err("超时：没有在终端输出里看到 probe-ok".into())
    }
}

fn tail(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        text
    } else {
        let mut start = text.len() - limit;
        while start < text.len() && !text.is_char_boundary(start) {
            start += 1;
        }
        &text[start..]
    }
}
