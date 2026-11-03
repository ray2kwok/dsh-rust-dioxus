//! 端到端探针：验证 `session/control` / `session/projections` / `subagents/*` 的线上形状。
//!
//! ```bash
//! DSH_BASE=http://127.0.0.1:3080/ DSH_TOKEN=… cargo run -p dsh-runtime --example subagent-probe
//! ```
//!
//! 不自己拉起 Host；`DSH_BASE` / `DSH_TOKEN` 来自 `dsh web` 的启动行。

use std::time::Duration;

use dsh_proto::session::{SessionControlFrame, SessionProjectionsValue};
use dsh_transport::{Credential, MuxConnection, RpcClient, StreamEvent};
use reqwest::Url;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base = Url::parse(&std::env::var("DSH_BASE")?)?;
    let token = std::env::var("DSH_TOKEN")?;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()?;
    let credential = Credential::acquire(&http, &base, &token).await?;
    let rpc = RpcClient::new(base.clone(), Some(&credential), Duration::from_secs(30))?;
    let mux = MuxConnection::connect(&base, Some(&credential)).await?;

    let list = rpc
        .call_value("session/list", json!({ "_request": {} }))
        .await?;
    let sessions = list
        .as_ref()
        .and_then(|value| value.get("items"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    println!("sessions = {}", sessions.len());
    let mut first = sessions
        .first()
        .and_then(|item| item.get("sessionId"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);

    if first.is_none() {
        let cwd = std::env::current_dir()?.to_string_lossy().to_string();
        let created = rpc
            .call_value("session/create", json!({ "request": { "cwd": cwd } }))
            .await?;
        first = created
            .as_ref()
            .and_then(|value| value.get("sessionId"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        println!("created session = {first:?}");
    }
    println!("first = {first:?}");

    if let Some(session_id) = first.clone() {
        match rpc
            .call::<SessionProjectionsValue>(
                "session/projections",
                json!({ "request": { "sessionId": session_id } }),
            )
            .await
        {
            Ok(value) => {
                let value = value.expect("projections 应有值");
                let keys = value
                    .values()
                    .as_object()
                    .map(|map| map.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                println!("projections OK; kind keys = {keys:?}");
                for key in [
                    "subagentCatalog",
                    "subagentTiming",
                    "subagent",
                    "tokenUsage",
                ] {
                    println!(
                        "  {key} = {}",
                        value
                            .values()
                            .get(key)
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "<absent>".to_string())
                    );
                }
            }
            Err(err) => println!("projections ERR = {err}"),
        }
    }

    let mut control = mux.open_args("session/control", json!({}))?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
    let mut frames = 0;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(left, control.next()).await {
            Ok(Some(StreamEvent::Frame(value))) => {
                frames += 1;
                match serde_json::from_value::<SessionControlFrame>(value.clone()) {
                    Ok(SessionControlFrame::Baseline { value }) => {
                        let sample = value.projections.iter().next().map(|(id, baseline)| {
                            (
                                id.clone(),
                                baseline
                                    .values
                                    .as_object()
                                    .map(|map| map.keys().cloned().collect::<Vec<_>>())
                                    .unwrap_or_default(),
                            )
                        });
                        println!(
                            "control baseline OK; sessions={} sample={sample:?}",
                            value.projections.len()
                        );
                    }
                    Ok(SessionControlFrame::Projection {
                        session_id,
                        key,
                        seq,
                        ..
                    }) => {
                        println!("control projection {session_id} {key} seq={seq}");
                    }
                    Err(err) => println!("control parse ERR = {err}; raw={value}"),
                }
            }
            Ok(Some(other)) => {
                println!("control stream event = {other:?}");
                break;
            }
            Ok(None) | Err(_) => break,
        }
    }
    println!("control frames = {frames}");

    if let Some(session_id) = first {
        let parent = session_id.clone();
        let child = "session-does-not-exist".to_string();
        let prompt = json!({
            "request": {
                "requestId": uuid::Uuid::new_v4().to_string(),
                "parentSessionId": parent,
                "childSessionId": child,
                "mode": "continuable",
                "delivery": "queue",
                "content": [{ "type": "text", "text": "probe" }]
            }
        });
        match rpc.call_value("subagents/prompt", prompt).await {
            Ok(value) => println!("subagents/prompt OK = {value:?}"),
            Err(err) => println!("subagents/prompt ERR = {err}"),
        }

        match rpc
            .call_value(
                "subagents/interruptByParent",
                json!({
                    "childSessionId": "session-does-not-exist",
                    "parentSessionId": session_id,
                    "mode": "continuable"
                }),
            )
            .await
        {
            Ok(value) => println!("subagents/interruptByParent OK = {value:?}"),
            Err(err) => println!("subagents/interruptByParent ERR = {err}"),
        }
    }

    control.cancel();
    Ok(())
}
