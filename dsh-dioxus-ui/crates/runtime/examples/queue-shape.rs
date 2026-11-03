//! 探测 `session/updateQueue` 的 `args` 形状。
//!
//! 网关的 `gateway/arguments-invalid` 会把 descriptor 期望的字段名回显出来
//! （`missing "x"; unexpected "y"`），据此逐步收敛，不用猜。

use dsh_runtime::DshClient;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = DshClient::launch().await?;

    let list = client.list_sessions().await?;
    let session = match list.first() {
        Some(item) => item.session_id.as_str().to_string(),
        None => {
            let cwd = std::env::current_dir()?.to_string_lossy().to_string();
            client
                .create_session(Some(cwd))
                .await?
                .session_id
                .as_str()
                .to_string()
        }
    };
    println!("session = {session}\n");

    let rpc = client.rpc();

    // 逐步试探：每次失败的错误信息会指出缺什么/多什么
    let attempts = [
        ("{}", json!({})),
        ("{request:{}}", json!({ "request": {} })),
        (
            "{request:{sessionId}}",
            json!({ "request": { "sessionId": session } }),
        ),
        (
            "{request:{sessionId,itemId,action:{kind:remove}}}",
            json!({ "request": { "sessionId": session, "itemId": "x", "action": { "kind": "remove" } } }),
        ),
    ];

    for (label, args) in attempts {
        match rpc.call_value("session/updateQueue", args).await {
            Ok(value) => println!("{label:<46} → OK {value:?}"),
            Err(err) => println!("{label:<46} → {err}"),
        }
    }

    client.shutdown().await;
    Ok(())
}
