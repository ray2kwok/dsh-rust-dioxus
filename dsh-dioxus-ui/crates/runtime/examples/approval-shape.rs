//! 校验审批回执的报文形状是否被网关接受。
//!
//! 没有模型时无法产生真实的 `approval/request`，但可以发一个**故意非法**的
//! `$events/result`：
//!
//! - 如果返回 `gateway/arguments-invalid` / `gateway/bad-request` → 我们的字段形状错了；
//! - 如果返回「找不到该事件」之类的业务错误 → 形状正确，只是 id 不存在。
//!
//! ```bash
//! DSH_BIN=./scripts/dev-host.sh cargo run -p dsh-runtime --example approval-shape
//! ```

use dsh_runtime::DshClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = DshClient::launch().await?;
    let mut events = client.open_events()?;
    let client_id = client
        .await_ready(&mut events)
        .await
        .ok_or("未收到 ready 帧")?;
    println!("clientId = {client_id}");

    let result = client
        .answer_approval(&client_id, "nonexistent-event-id", true)
        .await;

    match result {
        Ok(()) => println!("成功：形状被接受；Host 对不存在的 eventId 也返回成功（幂等语义）"),
        Err(err) => {
            let message = err.to_string();
            println!("返回: {message}");
            if message.contains("arguments-invalid") || message.contains("bad-request") {
                println!("→ 结论: 字段形状有问题");
            } else {
                println!("→ 结论: 形状被接受，仅事件不存在（符合预期）");
            }
        }
    }

    events.cancel();
    client.shutdown().await;
    Ok(())
}
