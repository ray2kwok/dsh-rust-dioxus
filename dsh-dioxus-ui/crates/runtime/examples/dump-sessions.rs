//! 端到端验证：拉起 Host → 列会话 → 建会话（如需）→ follow 快照 → 打印。
//!
//! 这是「mock → 真数据」的验收脚本，不依赖 GUI。
//!
//! ```bash
//! DSH_BIN=./scripts/dev-host.sh cargo run -p dsh-runtime --example dump-sessions
//! ```

use std::time::Duration;

use dsh_proto::session::SessionFollowFrame;
use dsh_runtime::DshClient;
use dsh_state::SessionView;
use dsh_transport::StreamEvent;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("== 拉起 Host 并握手 ==");
    let client = DshClient::launch().await?;
    println!("base_url = {}", client.base_url());

    println!("\n== $events ready ==");
    let mut events = client.open_events()?;
    match client.await_ready(&mut events).await {
        Some(client_id) => println!("clientId = {client_id}"),
        None => println!("未收到 ready"),
    }

    println!("\n== session/list ==");
    let mut sessions = client.list_sessions().await?;
    println!("现有会话 {} 个", sessions.len());

    if sessions.is_empty() {
        println!("\n== session/create（空列表时建一个）==");
        let cwd = std::env::current_dir()?.to_string_lossy().to_string();
        let created = client.create_session(Some(cwd)).await?;
        println!("created sessionId = {}", created.session_id);
        sessions = client.list_sessions().await?;
        println!("再次列出：{} 个", sessions.len());
    }

    let Some(first) = sessions.first() else {
        println!("没有会话可 follow");
        client.shutdown().await;
        return Ok(());
    };

    println!("\n== session/follow {} ==", first.session_id);
    println!(
        "  running={} blank={} agentAvailable={}",
        first.running, first.blank, first.agent_available
    );

    let mut stream = client.follow(first.session_id.as_str())?;
    let mut view = SessionView::new();
    let mut consumed = 0;
    for _ in 0..3 {
        match tokio::time::timeout(Duration::from_secs(10), stream.next()).await {
            Ok(Some(StreamEvent::Frame(value))) => {
                println!(
                    "  原始帧: {}",
                    value.to_string().chars().take(160).collect::<String>()
                );
                if let Ok(frame) = serde_json::from_value::<SessionFollowFrame>(value) {
                    view.apply(frame);
                    consumed += 1;
                }
            }
            Ok(Some(other)) => {
                println!("  流事件: {other:?}");
                break;
            }
            Ok(None) => {
                println!("  通道关闭");
                break;
            }
            Err(_) => {
                println!("  等待超时");
                break;
            }
        }
    }
    println!("  消费帧数 = {consumed}");
    println!("  entries = {}", view.len());
    println!("  cursor  = {}", view.cursor);
    if let Some(header) = &view.header {
        println!("  header  = id={} cwd={:?}", header.id, header.cwd);
    }

    stream.cancel();
    events.cancel();
    client.shutdown().await;
    println!("\ndone");
    Ok(())
}
