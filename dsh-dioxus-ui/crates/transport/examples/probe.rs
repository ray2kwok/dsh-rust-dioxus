//! 对真实 Host 做协议探测（M0.0）。
//!
//! 用途：把 `doc/PR.md` §12 列的开放问题一次性钉死——
//! 启动行格式、`/connect` 的 cookie 是否可用于一元调用、`session/list` 的 `args` 键名。
//!
//! 运行前提：PATH 里有 `dsh`（或设置 `DSH_BIN`）。
//!
//! ```bash
//! cargo run -p dsh-transport --example probe
//! ```

use std::time::Duration;

use dsh_proto::events::RemoteEventFrame;
use dsh_transport::{
    Credential, HostConfig, HostSupervisor, MuxConnection, RpcClient, StreamEvent,
};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = HostConfig::default();
    if let Ok(bin) = std::env::var("DSH_BIN") {
        config.bin = bin.into();
    }

    println!("== 1. 拉起 Host ==");
    let host = HostSupervisor::spawn(config).await?;
    let token_preview: String = host.token().chars().take(6).collect();
    println!("base_url = {}", host.base_url());
    println!("token    = {token_preview}…（长度 {}）", host.token().len());

    println!("\n== 2. /connect 换 cookie ==");
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()?;
    let credential = Credential::acquire(&http, host.base_url(), host.token()).await?;
    println!(
        "cookie   = {}…",
        credential
            .header_value()
            .chars()
            .take(24)
            .collect::<String>()
    );

    println!("\n== 3. 探测 session/list 的 args 键名 ==");
    let rpc = RpcClient::new(
        host.base_url().clone(),
        Some(&credential),
        Duration::from_secs(10),
    )?;
    for (label, args) in [
        ("{\"_request\":{}}", json!({ "_request": {} })),
        ("{\"request\":{}}", json!({ "request": {} })),
        ("{}", json!({})),
    ] {
        match rpc.call_value("session/list", args).await {
            Ok(value) => {
                let preview = value
                    .map(|v| v.to_string().chars().take(120).collect::<String>())
                    .unwrap_or_else(|| "<void>".into());
                println!("  {label:<18} → OK   {preview}");
            }
            Err(err) => println!("  {label:<18} → ERR  {err}"),
        }
    }

    println!("\n== 4. 打开 $events 并读取首帧 ==");
    let mux = MuxConnection::connect(host.base_url(), Some(&credential)).await?;
    let mut events = mux.open("$events", json!({ "args": {} }))?;
    match tokio::time::timeout(Duration::from_secs(10), events.next()).await {
        Ok(Some(StreamEvent::Frame(value))) => {
            match serde_json::from_value::<RemoteEventFrame>(value) {
                Ok(RemoteEventFrame::Ready { client_id, host }) => {
                    println!("  ready: clientId={client_id} home={}", host.home);
                }
                Ok(other) => println!("  首帧不是 ready: {other:?}"),
                Err(err) => println!("  首帧无法解析: {err}"),
            }
        }
        Ok(Some(StreamEvent::Error(err))) => println!("  流错误: {err}"),
        Ok(Some(StreamEvent::End)) => println!("  流已结束"),
        Ok(None) => println!("  通道关闭"),
        Err(_) => println!("  等待 ready 超时"),
    }

    println!("\n== 5. 关闭 Host ==");
    events.cancel();
    host.shutdown().await;
    println!("done");
    Ok(())
}
