//! dsh-dioxus-ui 桌面入口。
//!
//! 数据通路（与 `doc/ARCH.md` §8 一致）：
//!
//! ```text
//! 监督循环（use_future）
//!   ├─ DshClient::launch()     拉起 Host + 握手
//!   ├─ $events.ready           → generation；同时监听它判断断线
//!   ├─ session/list            左栏列表（空则自动建一个）
//!   └─ session/follow          SessionView.apply(frame) → 消息流
//! ```
//!
//! 断线策略：`$events` 流结束即视为连接失效，回到监督循环按 `Backoff`
//! （500ms→10s）重新拉起 Host 并重新加载，与上游「重连后整窗刷新」的语义一致。

mod map;

use std::sync::Arc;
use std::time::{Duration, Instant};

use dioxus::prelude::*;
use dsh_proto::session::SessionFollowFrame;
use dsh_runtime::DshClient;
use dsh_state::SessionView;
use dsh_transport::{Backoff, StreamEvent};
use dsh_ui::{AppShell, ConnectionState, MessageRow, SessionSummary};
use tokio::sync::mpsc;

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    dioxus::launch(App);
}

/// 界面共享信号。集中成一个结构，避免函数签名膨胀。
#[derive(Clone, Copy)]
struct Ui {
    connection: Signal<ConnectionState>,
    sessions: Signal<Vec<SessionSummary>>,
    messages: Signal<Vec<MessageRow>>,
    active: Signal<Option<String>>,
    host_label: Signal<String>,
    generation: Signal<u64>,
    client: Signal<Option<Arc<DshClient>>>,
}

#[component]
fn App() -> Element {
    let mut ui = Ui {
        connection: use_signal(|| ConnectionState::Connecting),
        sessions: use_signal(Vec::<SessionSummary>::new),
        messages: use_signal(Vec::<MessageRow>::new),
        active: use_signal(|| None::<String>),
        host_label: use_signal(|| "正在启动 Host…".to_string()),
        generation: use_signal(|| 0u64),
        client: use_signal(|| None::<Arc<DshClient>>),
    };

    // 监督循环：连上 → 等待断线 → 退避 → 再连
    use_future(move || async move {
        let mut backoff = Backoff::new();

        loop {
            ui.connection.set(ConnectionState::Connecting);
            ui.host_label.set("正在启动 Host…".to_string());

            match bootstrap(ui).await {
                Ok(disconnect_rx) => {
                    backoff.reset();
                    ui.connection.set(ConnectionState::Connected);

                    // 阻塞等到连接断开
                    let mut disconnect_rx = disconnect_rx;
                    let _ = disconnect_rx.recv().await;

                    tracing::warn!("连接已断开，准备重连");
                    ui.connection.set(ConnectionState::Disconnected);
                    ui.host_label.set("连接已断开，正在重连…".to_string());
                    ui.client.set(None);
                }
                Err(err) => {
                    tracing::error!(%err, "连接 Host 失败");
                    ui.connection.set(ConnectionState::Disconnected);
                    ui.host_label.set(format!("连接失败: {err}"));
                }
            }

            let delay = backoff.next_delay();
            tracing::info!(?delay, "退避后重试");
            tokio::time::sleep(delay).await;
        }
    });

    let on_select = move |id: String| {
        ui.active.set(Some(id.clone()));
        if let Some(handle) = ui.client.read().clone() {
            spawn_follow(handle, id, ui.messages);
        }
    };

    let on_send = move |text: String| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(session_id) = (ui.active)() else {
            return;
        };

        let local_id = format!("local-{}", ui.messages.read().len());
        ui.messages
            .write()
            .push(MessageRow::new(local_id, dsh_ui::Role::User, text.clone()));

        spawn(async move {
            if let Err(err) = handle.prompt(session_id, text).await {
                tracing::warn!(%err, "发送失败");
            }
        });
    };

    rsx! {
        document::Stylesheet { href: MAIN_CSS }
        AppShell {
            sessions: (ui.sessions)(),
            active_id: (ui.active)(),
            messages: (ui.messages)(),
            connection: (ui.connection)(),
            generation: (ui.generation)(),
            host_label: (ui.host_label)(),
            on_select,
            on_send,
        }
    }
}

/// 完成一次连接并返回「断线通知」通道。
async fn bootstrap(mut ui: Ui) -> Result<mpsc::Receiver<()>, dsh_transport::TransportError> {
    let handle = Arc::new(DshClient::launch().await?);
    ui.host_label.set(handle.base_url());
    tracing::info!(base = %handle.base_url(), "Host 握手完成");

    // 事件流同时承担两件事：取 generation、监听断线
    let (disconnect_tx, disconnect_rx) = mpsc::channel::<()>(1);
    if let Ok(mut events) = handle.open_events() {
        if handle.await_ready(&mut events).await.is_some() {
            ui.generation.set((ui.generation)() + 1);
            tracing::info!(generation = (ui.generation)(), "$events ready");
        }
        // 审批等 waterfall 在 M2 接入；当前先只做断线探测
        spawn(async move {
            while let Some(event) = events.next().await {
                if matches!(event, StreamEvent::End | StreamEvent::Error(_)) {
                    break;
                }
            }
            let _ = disconnect_tx.send(()).await;
        });
    }

    let mut list = handle.list_sessions().await?;
    tracing::info!(count = list.len(), "session/list 完成");

    if list.is_empty() {
        let cwd = std::env::current_dir()
            .ok()
            .map(|path| path.to_string_lossy().to_string());
        match handle.create_session(cwd).await {
            Ok(created) => {
                tracing::info!(session = %created.session_id, "已创建会话");
                list = handle.list_sessions().await.unwrap_or_default();
            }
            Err(err) => tracing::warn!(%err, "创建会话失败"),
        }
    }

    ui.sessions.set(list.iter().map(map::to_summary).collect());
    if let Some(first) = list.first() {
        let session_id = first.session_id.as_str().to_string();
        ui.active.set(Some(session_id.clone()));
        spawn_follow(Arc::clone(&handle), session_id, ui.messages);
    }

    ui.client.set(Some(handle));
    Ok(disconnect_rx)
}

/// 打开会话事件流，把帧归约进 `SessionView` 并刷新消息。
///
/// 「自动选中首个会话」与「用户点选」共用这一条路径，避免行为分叉。
fn spawn_follow(handle: Arc<DshClient>, session_id: String, mut messages: Signal<Vec<MessageRow>>) {
    spawn(async move {
        let Ok(mut stream) = handle.follow(session_id) else {
            return;
        };
        let mut view = SessionView::new();
        let mut announced = false;
        // 流式合批：高频 chunk 下把重渲压到 ~20fps，同时保证最后一定落地一次
        let mut last_flush = Instant::now();

        while let Some(event) = stream.next().await {
            match event {
                StreamEvent::Frame(value) => {
                    if let Ok(frame) = serde_json::from_value::<SessionFollowFrame>(value) {
                        view.apply(frame);
                        if !announced {
                            announced = true;
                            tracing::info!(
                                entries = view.len(),
                                cursor = view.cursor,
                                "follow 已就绪"
                            );
                        }
                        if last_flush.elapsed() >= Duration::from_millis(50) {
                            messages.set(map::rows(&view));
                            last_flush = Instant::now();
                        }
                    }
                }
                StreamEvent::Error(err) => {
                    tracing::warn!(%err, "follow 流出错");
                    break;
                }
                StreamEvent::End => break,
            }
        }

        // 收尾：把节流期间攒下的最后一帧补上
        messages.set(map::rows(&view));
    });
}

/// 保留给后续页面：把 wire 摘要批量转成视图摘要。
#[allow(dead_code)]
fn summarize(items: &[dsh_proto::session::SessionSummary]) -> Vec<SessionSummary> {
    items.iter().map(map::to_summary).collect()
}

/// 便于阅读的超时包装（诊断用）。
#[allow(dead_code)]
async fn with_timeout<T>(future: impl Future<Output = T>) -> Option<T> {
    tokio::time::timeout(Duration::from_secs(10), future)
        .await
        .ok()
}
