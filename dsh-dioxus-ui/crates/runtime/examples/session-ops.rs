//! 校验会话运维操作：创建 → 重命名 → 归档。
//!
//! 这些方法里，`session/rename` 的形状来自上游类型定义；
//! `workspace/archiveSession` 的 `args` 尚未实测，本示例用来确认。

use std::time::Duration;

use dsh_runtime::DshClient;
use dsh_transport::StreamEvent;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = DshClient::launch().await?;

    let cwd = std::env::current_dir()?.to_string_lossy().to_string();
    let created = client.create_session(Some(cwd)).await?;
    let session = created.session_id.as_str().to_string();
    println!("created  = {session}");

    let before = client.list_sessions().await?.len();
    println!("list before = {before}");

    match client.rename_session(&session, "冒烟测试会话").await {
        Ok(()) => println!("rename   = ok"),
        Err(err) => println!("rename   = ERR {err}"),
    }

    match client.archive_session(&session).await {
        Ok(()) => println!("archive  = ok"),
        Err(err) => println!("archive  = ERR {err}"),
    }

    // 归档不在 session/list 里体现，而在 workspace/follow 的 baseline
    let mut workspace = client.follow_workspace()?;
    if let Ok(Some(StreamEvent::Frame(value))) =
        tokio::time::timeout(Duration::from_secs(10), workspace.next()).await
    {
        let archived = DshClient::archived_ids(&value);
        println!("archived ids = {:?}", archived);
        println!(
            "our session archived = {}",
            archived.iter().any(|id| id == &session)
        );
    }
    workspace.cancel();

    let after = client.list_sessions().await?;
    println!("list after  = {}（归档后应从列表消失）", after.len());
    println!(
        "still visible = {}",
        after.iter().any(|item| item.session_id.as_str() == session)
    );

    client.shutdown().await;
    Ok(())
}
