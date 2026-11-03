//! dsh-dioxus-ui 桌面入口。
//!
//! ```text
//! 监督循环  DshClient::launch → $events.ready → session/list → session/follow
//!   ├─ 断线/手动重连 → Backoff → 重新拉起
//!   ├─ 审批 waterfall → 审批卡 → $events/result
//!   └─ 右栏文件树 / 预览 → workspaceFiles/*
//! ```

mod map;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dioxus::prelude::*;
use dsh_proto::events::RemoteEventFrame;
use dsh_proto::session::{SessionAddress, SessionControlFrame, SessionFollowFrame};
use dsh_runtime::DshClient;
use dsh_state::{ProjectionStore, SessionView};
use dsh_transport::{Backoff, StreamEvent};
use dsh_ui::{
    AppShell, ApprovalItem, AsideChat, ConnectionState, FileEntry, MessageRow, QuestionCard,
    QueueItem, SendMode, SessionSummary, SubagentRow, Theme, TrajectoryRow,
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
    /// 全部会话（含已归档），渲染时再按 `archived` 过滤
    all_sessions: Signal<Vec<SessionSummary>>,
    archived: Signal<Vec<String>>,
    messages: Signal<Vec<MessageRow>>,
    trajectory: Signal<Vec<TrajectoryRow>>,
    files: Signal<Vec<FileEntry>>,
    preview: Signal<Option<String>>,
    active: Signal<Option<String>>,
    /// 当前会话的完整地址（普通会话或子会话）。
    active_address: Signal<Option<SessionAddress>>,
    /// 子会话的祖先链 `(address, title)`，最后一项是直接的父会话。
    lineage: Signal<Vec<(SessionAddress, String)>>,
    /// 全 Host projection 缓存（`session/control`）。
    projections: Signal<ProjectionStore>,
    /// 已加载的子 Agent 目录，按父会话 id 缓存。
    subagent_tree: Signal<HashMap<String, Vec<SubagentRow>>>,
    /// 展开的目录节点。
    subagent_expanded: Signal<Vec<String>>,
    /// 右栏侧开的子会话。
    aside: Signal<Option<AsideChat>>,
    active_title: Signal<String>,
    host_label: Signal<String>,
    generation: Signal<u64>,
    client: Signal<Option<Arc<DshClient>>>,
    approvals: Signal<Vec<ApprovalItem>>,
    questions: Signal<Vec<QuestionCard>>,
    queue: Signal<Vec<QueueItem>>,
    context_usage: Signal<Option<(u64, u64)>>,
    has_more: Signal<bool>,
    view: Signal<Option<SessionView>>,
    client_id: Signal<Option<String>>,
    running: Signal<bool>,
    send_mode: Signal<SendMode>,
    theme: Signal<Theme>,
    locale: Signal<dsh_ui::Locale>,
    /// 手动重连触发器（监督循环里 select）。
    force_reconnect: Signal<Option<mpsc::UnboundedSender<()>>>,
}

#[component]
fn App() -> Element {
    let (force_tx, force_rx) = mpsc::unbounded_channel::<()>();
    // use_future 的闭包是 FnMut，Receiver 不能直接 move 进去：放进 Mutex 后取一次
    let force_rx = Arc::new(std::sync::Mutex::new(Some(force_rx)));

    // 语言信号既进 Ui，也通过 context 下发给组件（组件用 use_t() 读）
    let locale_signal = use_signal(|| match std::env::var("DSH_LOCALE").as_deref() {
        Ok("en") => dsh_ui::Locale::En,
        _ => dsh_ui::Locale::Zh,
    });
    use_context_provider(|| locale_signal);

    let mut ui = Ui {
        connection: use_signal(|| ConnectionState::Connecting),
        all_sessions: use_signal(Vec::<SessionSummary>::new),
        archived: use_signal(Vec::<String>::new),
        messages: use_signal(Vec::<MessageRow>::new),
        trajectory: use_signal(Vec::<TrajectoryRow>::new),
        files: use_signal(Vec::<FileEntry>::new),
        preview: use_signal(|| None::<String>),
        active: use_signal(|| None::<String>),
        active_address: use_signal(|| None::<SessionAddress>),
        lineage: use_signal(Vec::<(SessionAddress, String)>::new),
        projections: use_signal(ProjectionStore::new),
        subagent_tree: use_signal(HashMap::<String, Vec<SubagentRow>>::new),
        subagent_expanded: use_signal(Vec::<String>::new),
        aside: use_signal(|| None::<AsideChat>),
        active_title: use_signal(String::new),
        host_label: use_signal(|| "正在启动 Host…".to_string()),
        generation: use_signal(|| 0u64),
        client: use_signal(|| None::<Arc<DshClient>>),
        approvals: use_signal(Vec::<ApprovalItem>::new),
        questions: use_signal(Vec::<QuestionCard>::new),
        queue: use_signal(Vec::<QueueItem>::new),
        context_usage: use_signal(|| None::<(u64, u64)>),
        has_more: use_signal(|| false),
        view: use_signal(|| None::<SessionView>),
        client_id: use_signal(|| None::<String>),
        running: use_signal(|| false),
        send_mode: use_signal(|| SendMode::Queue),
        theme: use_signal(|| Theme::System),
        locale: locale_signal,
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
                        ui.questions.set(Vec::new());
                        ui.client_id.set(None);
                        ui.client.set(None);
                        ui.active_address.set(None);
                        ui.lineage.set(Vec::new());
                        ui.subagent_tree.set(HashMap::new());
                        ui.subagent_expanded.set(Vec::new());
                        ui.aside.set(None);
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

    let on_new_session = move |_| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        spawn(async move {
            let cwd = std::env::current_dir()
                .ok()
                .map(|path| path.to_string_lossy().to_string());
            match handle.create_session(cwd).await {
                Ok(created) => {
                    let id = created.session_id.as_str().to_string();
                    tracing::info!(%id, "新建会话");
                    if let Ok(list) = handle.list_sessions().await {
                        ui.all_sessions
                            .set(list.iter().map(map::to_summary).collect());
                    }
                    ui.active.set(Some(id.clone()));
                    ui.active_address
                        .set(Some(SessionAddress::session(id.clone())));
                    ui.lineage.set(Vec::new());
                    ui.messages.set(Vec::new());
                    ui.trajectory.set(Vec::new());
                    ui.queue.set(Vec::new());
                    ui.subagent_expanded.set(Vec::new());
                    ui.aside.set(None);
                    spawn_follow(Arc::clone(&handle), SessionAddress::session(id), ui);
                }
                Err(err) => tracing::warn!(%err, "新建会话失败"),
            }
        });
    };

    let on_select = move |id: String| {
        ui.active.set(Some(id.clone()));
        ui.active_address
            .set(Some(SessionAddress::session(id.clone())));
        ui.lineage.set(Vec::new());
        ui.subagent_expanded.set(Vec::new());
        ui.aside.set(None);
        ui.messages.set(Vec::new());
        ui.trajectory.set(Vec::new());
        ui.queue.set(Vec::new());
        ui.has_more.set(false);
        ui.running.set(false);
        if let Some(handle) = ui.client.read().clone() {
            spawn_follow(handle, SessionAddress::session(id), ui);
        }
    };

    let on_send = move |text: String| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(address) = (ui.active_address)() else {
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
            let result = match &address {
                SessionAddress::Subagent {
                    parent_session_id,
                    child_session_id,
                    mode: sub_mode,
                } if sub_mode.is_continuable() => handle
                    .prompt_subagent(
                        parent_session_id.as_str(),
                        child_session_id.as_str(),
                        *sub_mode,
                        text,
                        mode,
                    )
                    .await
                    .map(|_| ()),
                _ => {
                    handle
                        .prompt_with_mode(address.session_id().as_str().to_string(), text, mode)
                        .await
                }
            };
            if let Err(err) = result {
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

    let on_answer = move |(event_id, selected, custom): (String, Vec<String>, String)| {
        let card = ui
            .questions
            .read()
            .iter()
            .find(|card| card.event_id == event_id)
            .cloned();
        ui.questions.write().retain(|c| c.event_id != event_id);

        let (Some(card), Some(handle)) = (card, ui.client.read().clone()) else {
            return;
        };

        let mut answers = Vec::new();
        for item in &card.items {
            answers.push(serde_json::json!({
                "id": item.id,
                "selected": if selected.is_empty() { Vec::<String>::new() } else { selected.clone() },
                "custom": if custom.trim().is_empty() { serde_json::Value::Null } else { serde_json::Value::String(custom.clone()) },
            }));
        }
        // null 字段直接去掉更贴近上游（custom 可选）
        for answer in &mut answers {
            if let Some(obj) = answer.as_object_mut()
                && obj.get("custom").map(|v| v.is_null()).unwrap_or(false)
            {
                obj.remove("custom");
            }
        }
        let payload = serde_json::json!({ "answers": answers });

        let agent_id = card.agent_id.clone();
        let call_id = card.call_id.clone();
        spawn(async move {
            if let Err(err) = handle.answer_question(agent_id, call_id, payload).await {
                tracing::warn!(%err, "回答提交失败");
            }
        });
    };

    let on_rename = move |title: String| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(address) = (ui.active_address)() else {
            return;
        };
        if address.is_subagent() {
            return;
        }
        let session_id = address.session_id().as_str().to_string();
        ui.active_title.set(title.clone());
        spawn(async move {
            if let Err(err) = handle
                .rename_session(session_id.clone(), title.clone())
                .await
            {
                tracing::warn!(%err, "重命名失败");
                return;
            }
            if let Ok(list) = handle.list_sessions().await {
                // 归档信息来自 workspace/follow 的 baseline（session/list 不区分归档）
                if let Ok(mut workspace) = handle.follow_workspace() {
                    if let Ok(Some(StreamEvent::Frame(value))) =
                        tokio::time::timeout(Duration::from_secs(5), workspace.next()).await
                    {
                        let archived = DshClient::archived_ids(&value);
                        tracing::info!(count = archived.len(), "workspace baseline: 已归档会话");
                        ui.archived.set(archived);
                    }
                    workspace.cancel();
                }

                ui.all_sessions
                    .set(list.iter().map(map::to_summary).collect());
            }
        });
    };

    let on_archive = move |_| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(address) = (ui.active_address)() else {
            return;
        };
        if address.is_subagent() {
            return;
        }
        let session_id = address.session_id().as_str().to_string();
        ui.active.set(None);
        ui.active_address.set(None);
        ui.messages.set(Vec::new());
        ui.trajectory.set(Vec::new());
        spawn(async move {
            if let Err(err) = handle.archive_session(session_id).await {
                tracing::warn!(%err, "归档失败");
            }
            // 以 Host 的列表为准，不做本地乐观删除（归档是否影响 list 尚未确认）
            if let Ok(list) = handle.list_sessions().await {
                // 归档信息来自 workspace/follow 的 baseline（session/list 不区分归档）
                if let Ok(mut workspace) = handle.follow_workspace() {
                    if let Ok(Some(StreamEvent::Frame(value))) =
                        tokio::time::timeout(Duration::from_secs(5), workspace.next()).await
                    {
                        let archived = DshClient::archived_ids(&value);
                        tracing::info!(count = archived.len(), "workspace baseline: 已归档会话");
                        ui.archived.set(archived);
                    }
                    workspace.cancel();
                }

                ui.all_sessions
                    .set(list.iter().map(map::to_summary).collect());
            }
        });
    };

    let on_load_earlier = move |_| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(address) = (ui.active_address)() else {
            return;
        };
        let Some(mut current) = (ui.view)() else {
            return;
        };
        let Some(first_seq) = current.entries.first().map(|event| event.seq) else {
            return;
        };

        spawn(async move {
            match handle.page_before_address(address, first_seq, 200).await {
                Ok(page) => {
                    let events: Vec<_> = page
                        .records
                        .into_iter()
                        .map(|record| record.event)
                        .collect();
                    current.prepend(events, page.has_more);
                    tracing::info!(entries = current.len(), "已加载更早历史");
                    ui.messages.set(map::rows(&current));
                    ui.has_more.set(current.has_more);
                    ui.view.set(Some(current));
                }
                Err(err) => tracing::warn!(%err, "加载历史失败"),
            }
        });
    };

    let on_queue_action = move |(item_id, steer): (String, bool)| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(session_id) = (ui.active)() else {
            return;
        };
        // 乐观移除，失败时下次快照会纠正
        ui.queue.write().retain(|item| item.id != item_id);
        spawn(async move {
            let result = if steer {
                handle
                    .steer_queued(session_id.clone(), item_id.clone())
                    .await
            } else {
                handle
                    .remove_queued(session_id.clone(), item_id.clone())
                    .await
            };
            if let Err(err) = result {
                tracing::warn!(%err, "队列操作失败");
            }
        });
    };

    let on_cancel = move |_| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(address) = (ui.active_address)() else {
            return;
        };
        spawn(async move {
            let result = match &address {
                SessionAddress::Subagent {
                    parent_session_id,
                    child_session_id,
                    mode,
                } if mode.is_continuable() => handle
                    .interrupt_subagent(
                        parent_session_id.as_str(),
                        child_session_id.as_str(),
                        *mode,
                    )
                    .await
                    .map(|_| ()),
                _ => {
                    handle
                        .cancel(address.session_id().as_str().to_string())
                        .await
                }
            };
            if let Err(err) = result {
                tracing::warn!(%err, "取消失败");
            }
        });
    };

    let on_mode = move |mode: SendMode| ui.send_mode.set(mode);
    let on_theme = move |theme: Theme| ui.theme.set(theme);
    let on_locale = move |locale: dsh_ui::Locale| ui.locale.set(locale);
    let on_reconnect = move |_| {
        if let Some(tx) = (ui.force_reconnect)() {
            let _ = tx.send(());
        }
    };

    let on_subagent_refresh = move |_| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(address) = (ui.active_address)() else {
            return;
        };
        refresh_catalog(handle, address.session_id().as_str().to_string(), ui);
    };

    let on_subagent_toggle = move |id: String| {
        let mut expanded = (ui.subagent_expanded)();
        if let Some(pos) = expanded.iter().position(|item| item == &id) {
            expanded.remove(pos);
            ui.subagent_expanded.set(expanded);
            return;
        }
        expanded.push(id.clone());
        ui.subagent_expanded.set(expanded);
        if !(ui.subagent_tree)().contains_key(&id)
            && let Some(handle) = ui.client.read().clone()
        {
            refresh_catalog(handle, id, ui);
        }
    };

    let on_subagent_open = move |row: SubagentRow| {
        let Some(parent_address) = (ui.active_address)() else {
            return;
        };
        let parent_title = (ui.active_title)();
        ui.lineage
            .write()
            .push((parent_address.clone(), parent_title));
        let address = SessionAddress::subagent(
            parent_address.session_id().as_str().to_string(),
            row.id.clone(),
            map::wire_mode(row.mode),
        );
        ui.active.set(Some(row.id.clone()));
        ui.active_address.set(Some(address.clone()));
        ui.subagent_expanded.set(Vec::new());
        ui.aside.set(None);
        ui.messages.set(Vec::new());
        ui.trajectory.set(Vec::new());
        ui.queue.set(Vec::new());
        ui.has_more.set(false);
        ui.running.set(false);
        if let Some(handle) = ui.client.read().clone() {
            spawn_follow(handle, address, ui);
        }
    };

    let on_subagent_aside = move |row: SubagentRow| {
        let Some(parent_address) = (ui.active_address)() else {
            return;
        };
        let address = SessionAddress::subagent(
            parent_address.session_id().as_str().to_string(),
            row.id.clone(),
            map::wire_mode(row.mode),
        );
        ui.aside.set(Some(AsideChat {
            id: row.id.clone(),
            title: row.label.clone(),
            messages: Vec::new(),
            running: false,
        }));
        if let Some(handle) = ui.client.read().clone() {
            spawn_aside_follow(handle, address, ui);
        }
    };

    let on_navigate = move |id: String| {
        let lineage = (ui.lineage)();
        let Some(pos) = lineage
            .iter()
            .position(|(address, _)| address.session_id().as_str() == id)
        else {
            return;
        };
        let (address, _) = lineage[pos].clone();
        ui.lineage.set(lineage[..pos].to_vec());
        ui.active.set(Some(id));
        ui.active_address.set(Some(address.clone()));
        ui.subagent_expanded.set(Vec::new());
        ui.aside.set(None);
        ui.messages.set(Vec::new());
        ui.trajectory.set(Vec::new());
        ui.queue.set(Vec::new());
        ui.has_more.set(false);
        ui.running.set(false);
        if let Some(handle) = ui.client.read().clone() {
            spawn_follow(handle, address, ui);
        }
    };

    let on_close_aside = move |_| ui.aside.set(None);

    let on_open_file = move |path: String| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(address) = (ui.active_address)() else {
            return;
        };
        let scope = address.session_id().as_str().to_string();
        spawn(async move {
            match handle.read_file(&scope, &path).await {
                Ok(text) => ui.preview.set(Some(text)),
                Err(err) => ui.preview.set(Some(format!("读取失败: {err}"))),
            }
        });
    };

    let active_address = (ui.active_address)();
    let is_subagent = active_address
        .as_ref()
        .is_some_and(SessionAddress::is_subagent);
    let subagent_mode = active_address
        .as_ref()
        .and_then(SessionAddress::mode)
        .map(map::ui_mode);
    let parent_available = active_address
        .as_ref()
        .and_then(SessionAddress::parent_session_id)
        .map(|parent| {
            (ui.all_sessions)()
                .iter()
                .any(|summary| summary.id == parent.as_str())
        })
        .unwrap_or(true);
    // 目录成员直接从 projection store 读：control 的 baseline/增量能实时反映到
    // header，而不依赖「父会话是否已经缓存过目录」。
    let projection_store = (ui.projections)();
    let subagent_rows = (ui.active)()
        .and_then(|id| {
            projection_store
                .get(&id, "subagentCatalog")
                .map(|catalog| map::catalog_rows(&id, catalog, &projection_store))
        })
        .unwrap_or_default();
    let lineage = (ui.lineage)()
        .into_iter()
        .map(|(address, title)| (address.session_id().as_str().to_string(), title))
        .collect::<Vec<_>>();

    rsx! {
        document::Style { "{APP_CSS}" }
        AppShell {
            sessions: {
                let archived = (ui.archived)();
                (ui.all_sessions)()
                    .into_iter()
                    .filter(|item| !item.is_subagent && !archived.contains(&item.id))
                    .collect::<Vec<_>>()
            },
            active_id: (ui.active)(),
            active_title: (ui.active_title)(),
            messages: (ui.messages)(),
            trajectory: (ui.trajectory)(),
            files: (ui.files)(),
            preview: (ui.preview)(),
            connection: (ui.connection)(),
            approvals: (ui.approvals)(),
            questions: (ui.questions)(),
            queue: (ui.queue)(),
            context_usage: (ui.context_usage)(),
            has_more: (ui.has_more)(),
            running: (ui.running)(),
            send_mode: (ui.send_mode)(),
            theme: (ui.theme)(),
            generation: (ui.generation)(),
            host_label: (ui.host_label)(),
            is_subagent,
            subagent_mode,
            parent_available,
            lineage,
            subagent_rows,
            subagent_tree: (ui.subagent_tree)(),
            subagent_expanded: (ui.subagent_expanded)(),
            aside: (ui.aside)(),
            on_navigate,
            on_subagent_toggle,
            on_subagent_open,
            on_subagent_aside,
            on_subagent_refresh,
            on_close_aside,
            on_select,
            on_new_session,
            on_send,
            on_decide,
            on_answer,
            on_queue_action,
            on_load_earlier,
            on_rename,
            on_archive,
            on_cancel,
            on_mode,
            on_theme,
            on_locale,
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
    let mut questions = ui.questions;

    ui.projections.set(ProjectionStore::new());
    ui.subagent_tree.set(HashMap::new());
    ui.subagent_expanded.set(Vec::new());
    ui.lineage.set(Vec::new());
    ui.aside.set(None);

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
                            RemoteEventFrame::Waterfall {
                                event,
                                event_id,
                                agent_id,
                                request,
                            } if event == "user-questions/request" => {
                                tracing::info!(%event_id, %agent_id, "收到提问请求");
                                questions
                                    .write()
                                    .push(map::question(event_id, agent_id, &request));
                            }
                            RemoteEventFrame::Cancel { event_id } => {
                                approvals.write().retain(|item| item.event_id != event_id);
                                questions.write().retain(|card| card.event_id != event_id);
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

    // 子 Agent 状态的实时来源：baseline 全量替换 + projection 增量。
    if let Ok(mut control) = handle.open_control() {
        let mut projections = ui.projections;
        let mut tree = ui.subagent_tree;
        spawn(async move {
            while let Some(event) = control.next().await {
                match event {
                    StreamEvent::Frame(value) => {
                        let Ok(frame) = serde_json::from_value::<SessionControlFrame>(value) else {
                            continue;
                        };
                        projections.write().apply_control(&frame);

                        // 只重算已经加载过的目录；未展开的分支等展开时再读。
                        let store = projections.read().clone();
                        let mut tree = tree.write();
                        for (parent_id, rows) in tree.iter_mut() {
                            if let Some(catalog) = store.get(parent_id, "subagentCatalog") {
                                *rows = map::catalog_rows(parent_id, catalog, &store);
                            }
                        }
                    }
                    StreamEvent::Error(err) => {
                        tracing::warn!(%err, "session/control 流出错");
                        break;
                    }
                    StreamEvent::End => break,
                }
            }
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

    // 归档信息来自 workspace/follow 的 baseline（session/list 不区分归档）
    if let Ok(mut workspace) = handle.follow_workspace() {
        if let Ok(Some(StreamEvent::Frame(value))) =
            tokio::time::timeout(Duration::from_secs(5), workspace.next()).await
        {
            let archived = DshClient::archived_ids(&value);
            tracing::info!(count = archived.len(), "workspace baseline: 已归档会话");
            ui.archived.set(archived);
        }
        workspace.cancel();
    }

    ui.all_sessions
        .set(list.iter().map(map::to_summary).collect());
    let archived_now = (ui.archived)();
    let first_visible = list.iter().find(|item| {
        item.origin.as_deref() != Some("subagent")
            && !archived_now.contains(&item.session_id.as_str().to_string())
    });
    if let Some(first) = first_visible {
        let session_id = first.session_id.as_str().to_string();
        ui.active.set(Some(session_id.clone()));
        ui.active_address
            .set(Some(SessionAddress::session(session_id.clone())));
        spawn_follow(
            Arc::clone(&handle),
            SessionAddress::session(session_id.clone()),
            ui,
        );

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

    if std::env::var("DSH_FAKE_QUESTION").is_ok() {
        tracing::info!("已注入调试提问卡");
        ui.questions.set(vec![QuestionCard {
            event_id: "fake-q1".to_string(),
            agent_id: "session-fake".to_string(),
            call_id: "call-fake".to_string(),
            items: vec![dsh_ui::QuestionItemView {
                id: "q1".to_string(),
                question: "要用哪种方式迁移样式？".to_string(),
                detail: Some("上游有 59 个 ui-* 包，逐个迁移还是先做骨架？".to_string()),
                options: vec!["逐屏深挖".to_string(), "先骨架后细节".to_string()],
                multi_select: false,
            }],
        }]);
    }

    // 调试：注入一棵假目录，便于无模型时验证目录/耗时/用量渲染。
    if std::env::var("DSH_FAKE_SUBAGENT").is_ok()
        && let Some(parent) = (ui.active)()
    {
        let mut projections = ui.projections;
        spawn(async move {
            // 等 control baseline 落地后再注入，避免被整窗替换掉。
            tokio::time::sleep(Duration::from_millis(1200)).await;
            projections.write().seed(
                    &parent,
                    serde_json::json!({
                        "title": "父会话",
                        "subagentCatalog": [
                            { "id": "session-fake-worker", "createdAt": 1, "mode": "continuable", "label": "worker" },
                            { "id": "session-fake-review", "createdAt": 2, "mode": "one-shot", "label": "reviewer" },
                            { "id": "session-fake-ext", "createdAt": 3, "mode": "external", "label": "external-task" }
                        ]
                    }),
                );
            projections.write().seed(
                "session-fake-worker",
                serde_json::json!({
                    "subagentTiming": {
                        "settledMs": 4200,
                        "active": { "since": 1000, "through": 3000 },
                        "lastTurnCompleted": false
                    },
                    "tokenUsage": {
                        "uncachedInputTokens": 1200,
                        "outputTokens": 340,
                        "cacheReadTokens": 800,
                        "cacheWriteTokens": 0
                    }
                }),
            );
            projections.write().seed(
                "session-fake-review",
                serde_json::json!({
                    "subagentTiming": { "settledMs": 8000, "lastTurnCompleted": true },
                    "tokenUsage": { "uncachedInputTokens": 500, "outputTokens": 120 }
                }),
            );
        });
    }

    ui.client.set(Some(handle));
    Ok(disconnect_rx)
}

/// 打开会话事件流并归约。
fn spawn_follow(handle: Arc<DshClient>, address: SessionAddress, ui: Ui) {
    let session_id = address.session_id().as_str().to_string();
    let is_subagent = address.is_subagent();
    let mut projections = ui.projections;
    let mut subagent_tree = ui.subagent_tree;
    let mut messages = ui.messages;
    let mut trajectory = ui.trajectory;
    let mut running = ui.running;
    let mut active_title = ui.active_title;
    let mut queue = ui.queue;
    let mut context_usage = ui.context_usage;
    let mut has_more = ui.has_more;
    let mut shared_view = ui.view;
    let active = ui.active;

    spawn(async move {
        let Ok(mut stream) = handle.follow_address(address) else {
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

                    let is_active = active.read().as_deref() == Some(session_id.as_str());

                    if !announced {
                        announced = true;
                        tracing::info!(
                            entries = view.len(),
                            cursor = view.cursor,
                            %session_id,
                            "follow 已就绪"
                        );
                        let title = if is_subagent {
                            map::subagent_title(&view).unwrap_or_else(|| session_id.clone())
                        } else {
                            let fallback = view
                                .header
                                .as_ref()
                                .and_then(|header| header.cwd.as_deref())
                                .and_then(|cwd| cwd.rsplit('/').find(|seg| !seg.is_empty()))
                                .unwrap_or("会话")
                                .to_string();
                            map::title(&view).unwrap_or(fallback)
                        };
                        if is_active {
                            active_title.set(title);
                        }

                        // 用 snapshot 种下 projection；父会话顺带初始化目录。
                        // 这部分与当前选中会话无关，重连后也需要。
                        projections
                            .write()
                            .seed(&session_id, view.projections.clone());
                        if let Some(catalog) = view.projection("subagentCatalog") {
                            let store = projections.read().clone();
                            let rows = map::catalog_rows(&session_id, catalog, &store);
                            subagent_tree.write().insert(session_id.clone(), rows);
                        }
                    }

                    if last_flush.elapsed() >= Duration::from_millis(50) {
                        if is_active {
                            running.set(view.live.is_some());
                            messages.set(map::rows(&view));
                            trajectory.set(map::trajectory(&view));
                            queue.set(map::queue_items(&view));
                            context_usage.set(map::context_usage(&view));
                            has_more.set(view.has_more);
                            shared_view.set(Some(view.clone()));
                        }
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

        if active.read().as_deref() == Some(session_id.as_str()) {
            running.set(false);
            messages.set(map::rows(&view));
            trajectory.set(map::trajectory(&view));
        }
    });
}

/// 读取某个父会话的 projection，并刷新它的目录缓存。
fn refresh_catalog(handle: Arc<DshClient>, parent_id: String, ui: Ui) {
    let mut projections = ui.projections;
    let mut tree = ui.subagent_tree;

    spawn(async move {
        match handle.refresh_projections(&parent_id).await {
            Ok(value) => {
                projections.write().seed_result(&parent_id, &value);
                let store = projections.read().clone();
                if let Some(catalog) = store.get(&parent_id, "subagentCatalog") {
                    let rows = map::catalog_rows(&parent_id, catalog, &store);
                    tree.write().insert(parent_id.clone(), rows);
                }
            }
            Err(err) => tracing::warn!(%err, %parent_id, "子 Agent 目录加载失败"),
        }
    });
}

/// 在右栏侧开一个子会话（只读副本，续聊仍在主面板）。
fn spawn_aside_follow(handle: Arc<DshClient>, address: SessionAddress, ui: Ui) {
    let mut aside = ui.aside;
    let session_id = address.session_id().as_str().to_string();

    spawn(async move {
        let Ok(mut stream) = handle.follow_address(address) else {
            return;
        };
        let mut view = SessionView::new();
        let mut last_flush = Instant::now();

        while let Some(event) = stream.next().await {
            match event {
                StreamEvent::Frame(value) => {
                    let Ok(frame) = serde_json::from_value::<SessionFollowFrame>(value) else {
                        continue;
                    };
                    view.apply(frame);

                    if last_flush.elapsed() >= Duration::from_millis(100) {
                        publish_aside(&mut aside, &session_id, &view);
                        last_flush = Instant::now();
                    }
                }
                StreamEvent::Error(err) => {
                    tracing::warn!(%err, "右栏子会话流出错");
                    break;
                }
                StreamEvent::End => break,
            }
        }

        publish_aside(&mut aside, &session_id, &view);
    });
}

/// 只在右栏仍展示该会话时写入，避免旧流覆盖后来打开的会话。
fn publish_aside(aside: &mut Signal<Option<AsideChat>>, session_id: &str, view: &SessionView) {
    let still_current = aside
        .read()
        .as_ref()
        .map(|chat| chat.id == session_id)
        .unwrap_or(false);
    if !still_current {
        return;
    }
    let title = map::subagent_title(view).unwrap_or_else(|| session_id.to_string());
    aside.set(Some(AsideChat {
        id: session_id.to_string(),
        title,
        messages: map::rows(view),
        running: view.live.is_some(),
    }));
}
