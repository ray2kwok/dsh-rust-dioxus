//! 探测 workspace/follow 的首帧，确认归档会话如何标记。
use dsh_runtime::DshClient;
use dsh_transport::StreamEvent;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = DshClient::launch().await?;
    let mut ws = client.follow_workspace()?;
    match tokio::time::timeout(Duration::from_secs(10), ws.next()).await {
        Ok(Some(StreamEvent::Frame(value))) => {
            let text = value.to_string();
            println!("首帧长度 = {}", text.len());
            println!("{}", text.chars().take(800).collect::<String>());
        }
        other => println!("未拿到首帧: {other:?}"),
    }
    ws.cancel();
    client.shutdown().await;
    Ok(())
}
