//! dsh-dioxus-ui 桌面入口。
//!
//! ```text
//! 监督循环  DshClient::launch → $events.ready → session/list → session/follow
//!   ├─ 断线/手动重连 → Backoff → 重新拉起
//!   ├─ 审批 waterfall → 审批卡 → $events/result
//!   └─ 右栏文件树 / 预览 → workspaceFiles/*
//! ```

mod map;

use std::sync::Arc;
use std::time::{Duration, Instant};

use dioxus::prelude::*;
use dsh_proto::events::RemoteEventFrame;
use dsh_proto::session::SessionFollowFrame;
use dsh_runtime::DshClient;
use dsh_state::SessionView;
use dsh_transport::{Backoff, StreamEvent};
use dsh_ui::{
    AppShell, ApprovalItem, ConnectionState, FileEntry, MessageRow, SendMode, SessionSummary,
    Theme, TrajectoryRow,
};
use tokio::sync::mpsc;

/// 全部样式在**编译期**拼进二进制。
///
/// 不用 `asset!` + `<link>`：debug 构建下 `asset!` 给的是绝对文件路径，
/// WebView 取不到（表现为完全无样式）。内联还有一个好处——产物真正单文件。
const APP_CSS: &str = concat!(
    include_str!("../assets/theme/base.css"),
    "\n",
    include_str!("../assets/theme/design-platform.css"),
    "\n",
    include_str!("../assets/theme/scrollbar.css"),
    "\n",
    include_str!("../assets/theme/focus.css"),
    "\n",
    include_str!("../assets/theme/corner-shape.css"),
    "\n",
    include_str!("../assets/main.css"),
);

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let window = dioxus::desktop::WindowBuilder::new()
        .with_title("dsh")
        .with_inner_size(dioxus::desktop::LogicalSize::new(1280.0, 860.0))
        .with_min_inner_size(dioxus::desktop::LogicalSize::new(960.0, 600.0));

    let config = dioxus::desktop::Config::new()
        .with_window(window)
        .with_menu(None);

    dioxus::LaunchBuilder::new().with_cfg(config).launch(App);
}

#[derive(Clone, Copy)]
struct Ui {
    connection: Signal<ConnectionState>,
    sessions: Signal<Vec<SessionSummary>>,
    messages: Signal<Vec<MessageRow>>,
    trajectory: Signal<Vec<TrajectoryRow>>,
    files: Signal<Vec<FileEntry>>,
    preview: Signal<Option<String>>,
    active: Signal<Option<String>>,
    active_title: Signal<String>,
    host_label: Signal<String>,
    generation: Signal<u64>,
    client: Signal<Option<Arc<DshClient>>>,
    approvals: Signal<Vec<ApprovalItem>>,
    client_id: Signal<Option<String>>,
    running: Signal<bool>,
    send_mode: Signal<SendMode>,
    theme: Signal<Theme>,
    /// 手动重连触发器（监督循环里 select）。
    force_reconnect: Signal<Option<mpsc::UnboundedSender<()>>>,
}

#[component]
fn App() -> Element {
    let (force_tx, force_rx) = mpsc::unbounded_channel::<()>();
    // use_future 的闭包是 FnMut，Receiver 不能直接 move 进去：放进 Mutex 后取一次
    let force_rx = Arc::new(std::sync::Mutex::new(Some(force_rx)));

    let mut ui = Ui {
        connection: use_signal(|| ConnectionState::Connecting),
        sessions: use_signal(Vec::<SessionSummary>::new),
        messages: use_signal(Vec::<MessageRow>::new),
        trajectory: use_signal(Vec::<TrajectoryRow>::new),
        files: use_signal(Vec::<FileEntry>::new),
        preview: use_signal(|| None::<String>),
        active: use_signal(|| None::<String>),
        active_title: use_signal(String::new),
        host_label: use_signal(|| "正在启动 Host…".to_string()),
        generation: use_signal(|| 0u64),
        client: use_signal(|| None::<Arc<DshClient>>),
        approvals: use_signal(Vec::<ApprovalItem>::new),
        client_id: use_signal(|| None::<String>),
        running: use_signal(|| false),
        send_mode: use_signal(|| SendMode::Queue),
        theme: use_signal(|| Theme::System),
        force_reconnect: use_signal(|| Some(force_tx)),
    };

    // 主题：解析 system → 在 <body> 上挂 data-ds-dark-theme（与上游 theme-presenter 一致）
    use_effect(move || {
        let attr = (ui.theme)().attribute();
        let _ = document::eval(&format!(
            r#"const theme = '{attr}';
const dark = theme === 'dark' || (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
if (dark) document.body.setAttribute('data-ds-dark-theme', '');
else document.body.removeAttribute('data-ds-dark-theme');
document.documentElement.style.colorScheme = dark ? 'dark' : 'light';"#
        ));
    });

    // 监督循环
    use_future(move || {
        let force_rx = Arc::clone(&force_rx);
        async move {
            let mut backoff = Backoff::new();
            let mut force_rx = force_rx.lock().unwrap().take();

            loop {
                ui.connection.set(ConnectionState::Connecting);
                ui.host_label.set("正在启动 Host…".to_string());

                match bootstrap(ui).await {
                    Ok(disconnect_rx) => {
                        backoff.reset();
                        ui.connection.set(ConnectionState::Connected);

                        let mut disconnect_rx = disconnect_rx;
                        match force_rx.as_mut() {
                            Some(force) => {
                                tokio::select! {
                                    _ = disconnect_rx.recv() => tracing::warn!("连接已断开，准备重连"),
                                    _ = force.recv() => tracing::warn!("手动重连"),
                                }
                            }
                            None => {
                                let _ = disconnect_rx.recv().await;
                                tracing::warn!("连接已断开，准备重连");
                            }
                        }

                        ui.approvals.set(Vec::new());
                        ui.client_id.set(None);
                        ui.client.set(None);
                        ui.connection.set(ConnectionState::Disconnected);
                        ui.host_label.set("连接已断开，正在重连…".to_string());
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
        }
    });

    let on_select = move |id: String| {
        ui.active.set(Some(id.clone()));
        if let Some(handle) = ui.client.read().clone() {
            spawn_follow(handle, id, ui);
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

        let mode = match (ui.send_mode)() {
            SendMode::Queue => dsh_proto::session::PromptMode::Queue,
            SendMode::Steer => dsh_proto::session::PromptMode::Steer,
        };
        spawn(async move {
            if let Err(err) = handle.prompt_with_mode(session_id, text, mode).await {
                tracing::warn!(%err, "发送失败");
            }
        });
    };

    let on_decide = move |(event_id, allow): (String, bool)| {
        ui.approvals
            .write()
            .retain(|item| item.event_id != event_id);
        if event_id.starts_with("fake-") {
            return;
        }
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(client_id) = (ui.client_id)() else {
            return;
        };
        spawn(async move {
            if let Err(err) = handle.answer_approval(client_id, event_id, allow).await {
                tracing::warn!(%err, "审批回执失败");
            }
        });
    };

    let on_cancel = move |_| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(session_id) = (ui.active)() else {
            return;
        };
        spawn(async move {
            if let Err(err) = handle.cancel(session_id).await {
                tracing::warn!(%err, "取消失败");
            }
        });
    };

    let on_mode = move |mode: SendMode| ui.send_mode.set(mode);
    let on_theme = move |theme: Theme| ui.theme.set(theme);
    let on_reconnect = move |_| {
        if let Some(tx) = (ui.force_reconnect)() {
            let _ = tx.send(());
        }
    };

    let on_open_file = move |path: String| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(scope) = (ui.active)() else {
            return;
        };
        spawn(async move {
            match handle.read_file(&scope, &path).await {
                Ok(text) => ui.preview.set(Some(text)),
                Err(err) => ui.preview.set(Some(format!("读取失败: {err}"))),
            }
        });
    };

    rsx! {
        document::Style { "{APP_CSS}" }
        AppShell {
            sessions: (ui.sessions)(),
            active_id: (ui.active)(),
            active_title: (ui.active_title)(),
            messages: (ui.messages)(),
            trajectory: (ui.trajectory)(),
            files: (ui.files)(),
            preview: (ui.preview)(),
            connection: (ui.connection)(),
            approvals: (ui.approvals)(),
            running: (ui.running)(),
            send_mode: (ui.send_mode)(),
            theme: (ui.theme)(),
            generation: (ui.generation)(),
            host_label: (ui.host_label)(),
            on_select,
            on_send,
            on_decide,
            on_cancel,
            on_mode,
            on_theme,
            on_reconnect,
            on_open_file,
        }
    }
}

/// 一次连接：拉起 Host → 事件流 → 会话列表 → 首会话 follow → 文件树。
async fn bootstrap(mut ui: Ui) -> Result<mpsc::Receiver<()>, dsh_transport::TransportError> {
    let handle = Arc::new(DshClient::launch().await?);
    ui.host_label.set(handle.base_url());
    tracing::info!(base = %handle.base_url(), "Host 握手完成");

    let (disconnect_tx, disconnect_rx) = mpsc::channel::<()>(1);
    let mut approvals = ui.approvals;

    if let Ok(mut events) = handle.open_events() {
        if let Some(client_id) = handle.await_ready(&mut events).await {
            ui.generation.set((ui.generation)() + 1);
            ui.client_id.set(Some(client_id.clone()));
            tracing::info!(generation = (ui.generation)(), %client_id, "$events ready");
        }

        spawn(async move {
            while let Some(event) = events.next().await {
                match event {
                    StreamEvent::Frame(value) => {
                        let Ok(frame) = serde_json::from_value::<RemoteEventFrame>(value) else {
                            continue;
                        };
                        match frame {
                            RemoteEventFrame::Waterfall {
                                event,
                                event_id,
                                agent_id,
                                request,
                            } if event == "approval/request" => {
                                tracing::info!(%event_id, %agent_id, "收到审批请求");
                                approvals
                                    .write()
                                    .push(map::approval(event_id, agent_id, &request));
                            }
                            RemoteEventFrame::Cancel { event_id } => {
                                approvals.write().retain(|item| item.event_id != event_id);
                            }
                            _ => {}
                        }
                    }
                    StreamEvent::End | StreamEvent::Error(_) => break,
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
        spawn_follow(Arc::clone(&handle), session_id.clone(), ui);

        // 右栏文件树：path 不能为空（网关返回 "path is required"），用会话 cwd 作根
        let root = first.cwd.clone().unwrap_or_else(|| ".".to_string());
        match handle.list_files(&session_id, &root).await {
            Ok(entries) => {
                tracing::info!(count = entries.len(), "文件树已加载");
                ui.files.set(
                    entries
                        .iter()
                        .map(|item| FileEntry {
                            path: item.path.clone(),
                            name: item.name.clone(),
                            is_dir: item.is_dir,
                        })
                        .collect(),
                );
            }
            Err(err) => tracing::warn!(%err, "文件树加载失败"),
        }
    }

    if std::env::var("DSH_FAKE_APPROVAL").is_ok() {
        tracing::info!("已注入调试审批卡");
        ui.approvals.set(vec![ApprovalItem {
            event_id: "fake-1".to_string(),
            agent_id: "session-fake".to_string(),
            title: "bash 请求执行".to_string(),
            detail: "{\"tool\":\"bash\",\"input\":{\"cmd\":\"rm -rf ./dist\"}}".to_string(),
        }]);
    }

    ui.client.set(Some(handle));
    Ok(disconnect_rx)
}

/// 打开会话事件流并归约。
fn spawn_follow(handle: Arc<DshClient>, session_id: String, ui: Ui) {
    let mut messages = ui.messages;
    let mut trajectory = ui.trajectory;
    let mut running = ui.running;
    let mut active_title = ui.active_title;

    spawn(async move {
        let Ok(mut stream) = handle.follow(session_id) else {
            return;
        };
        let mut view = SessionView::new();
        let mut announced = false;
        let mut last_flush = Instant::now();

        while let Some(event) = stream.next().await {
            match event {
                StreamEvent::Frame(value) => {
                    let Ok(frame) = serde_json::from_value::<SessionFollowFrame>(value) else {
                        continue;
                    };
                    view.apply(frame);

                    if !announced {
                        announced = true;
                        tracing::info!(entries = view.len(), cursor = view.cursor, "follow 已就绪");
                        if let Some(header) = &view.header
                            && let Some(cwd) = &header.cwd
                        {
                            active_title.set(
                                cwd.rsplit('/')
                                    .find(|seg| !seg.is_empty())
                                    .unwrap_or("会话")
                                    .to_string(),
                            );
                        }
                    }

                    running.set(view.live.is_some());
                    if last_flush.elapsed() >= Duration::from_millis(50) {
                        messages.set(map::rows(&view));
                        trajectory.set(map::trajectory(&view));
                        last_flush = Instant::now();
                    }
                }
                StreamEvent::Error(err) => {
                    tracing::warn!(%err, "follow 流出错");
                    break;
                }
                StreamEvent::End => break,
            }
        }

        running.set(false);
        messages.set(map::rows(&view));
        trajectory.set(map::trajectory(&view));
    });
}
