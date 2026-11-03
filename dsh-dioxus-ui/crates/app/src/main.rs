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

use base64::Engine;
use dioxus::prelude::*;
use dsh_proto::events::RemoteEventFrame;
use dsh_proto::interaction::{
    AgentPresetRoster, CommandDescriptor, FileReferenceCandidate, MessageFeedbackPutRequest,
    ModelCatalog, ModelSelection, PermissionCatalog,
};
use dsh_proto::session::{PromptContent, SessionAddress, SessionControlFrame, SessionFollowFrame};
use dsh_proto::settings::{LOCALE_NS, SUBAGENT_NS, SettingsDescribeValue, THEME_NS};
use dsh_proto::workspace::{WorkspaceFollowFrame, WorkspaceView};
use dsh_runtime::DshClient;
use dsh_state::{ProjectionStore, SessionView};
use dsh_transport::{Backoff, StreamEvent};
use dsh_ui::{
    AppShell, ApprovalItem, AsideChat, ConnectionState, FileEntry, MessageRow, QuestionCard,
    QueueItem, SearchHit, SendMode, SendRequest, SessionSummary, SettingsChange, SubagentRow,
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
        .with_menu(None)
        .with_custom_head(
            r#"<script>
try {
  const saved = localStorage.getItem('dsh-theme') || '';
  const dark = saved === 'dark' || (saved !== 'light' && window.matchMedia('(prefers-color-scheme: dark)').matches);
  if (dark) document.documentElement.setAttribute('data-ds-boot-dark', '');
} catch (_error) {}
</script>
<style>
html[data-ds-boot-dark] { background: #0f1115; color-scheme: dark; }
html { background: #ffffff; }
</style>"#
                .to_string(),
        );

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
    file_children: Signal<HashMap<String, Vec<FileEntry>>>,
    file_expanded: Signal<Vec<String>>,
    file_loading: Signal<bool>,
    file_root: Signal<String>,
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
    subagent_error: Signal<Option<String>>,
    subagent_loading: Signal<bool>,
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
    /// `workspace/follow` 的工作区、归档与置顶状态。
    workspaces: Signal<Vec<WorkspaceView>>,
    pinned: Signal<Vec<String>>,
    search_query: Signal<String>,
    search_results: Signal<Vec<SearchHit>>,
    searching: Signal<bool>,
    settings: Signal<Option<SettingsDescribeValue>>,
    settings_saving: Signal<bool>,
    models: Signal<Option<ModelCatalog>>,
    permissions: Signal<Option<PermissionCatalog>>,
    agent_presets: Signal<Option<AgentPresetRoster>>,
    selected_model: Signal<Option<ModelSelection>>,
    selected_permission: Signal<Option<String>>,
    default_preset: Signal<Option<String>>,
    commands: Signal<Vec<CommandDescriptor>>,
    file_refs: Signal<Vec<FileReferenceCandidate>>,
    error: Signal<Option<String>>,
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
        file_children: use_signal(HashMap::<String, Vec<FileEntry>>::new),
        file_expanded: use_signal(Vec::<String>::new),
        file_loading: use_signal(|| false),
        file_root: use_signal(String::new),
        preview: use_signal(|| None::<String>),
        active: use_signal(|| None::<String>),
        active_address: use_signal(|| None::<SessionAddress>),
        lineage: use_signal(Vec::<(SessionAddress, String)>::new),
        projections: use_signal(ProjectionStore::new),
        subagent_tree: use_signal(HashMap::<String, Vec<SubagentRow>>::new),
        subagent_expanded: use_signal(Vec::<String>::new),
        subagent_error: use_signal(|| None::<String>),
        subagent_loading: use_signal(|| false),
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
        workspaces: use_signal(Vec::<WorkspaceView>::new),
        pinned: use_signal(Vec::<String>::new),
        search_query: use_signal(String::new),
        search_results: use_signal(Vec::<SearchHit>::new),
        searching: use_signal(|| false),
        settings: use_signal(|| None::<SettingsDescribeValue>),
        settings_saving: use_signal(|| false),
        models: use_signal(|| None::<ModelCatalog>),
        permissions: use_signal(|| None::<PermissionCatalog>),
        agent_presets: use_signal(|| None::<AgentPresetRoster>),
        selected_model: use_signal(|| None::<ModelSelection>),
        selected_permission: use_signal(|| None::<String>),
        default_preset: use_signal(|| None::<String>),
        commands: use_signal(Vec::<CommandDescriptor>::new),
        file_refs: use_signal(Vec::<FileReferenceCandidate>::new),
        error: use_signal(|| None::<String>),
        force_reconnect: use_signal(|| Some(force_tx)),
    };

    // 主题/字号/字体：解析 system → 在 <body> 上挂 data-ds-dark-theme 与 CSS 变量。
    use_effect(move || {
        let theme = (ui.theme)();
        let settings = (ui.settings)();
        apply_appearance(&theme, &settings);
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
                        ui.subagent_error.set(None);
                        ui.subagent_loading.set(false);
                        ui.subagent_error.set(None);
                        ui.subagent_loading.set(false);
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

    let on_new_session = move |workspace_id: Option<String>| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let preset = (ui.default_preset)();
        let mut error = ui.error;
        spawn(async move {
            let cwd = if workspace_id.is_some() {
                None
            } else {
                std::env::current_dir()
                    .ok()
                    .map(|path| path.to_string_lossy().to_string())
            };
            match handle.create_session_full(workspace_id, cwd, preset).await {
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
                    ui.subagent_error.set(None);
                    ui.subagent_loading.set(false);
                    ui.aside.set(None);
                    spawn_follow(Arc::clone(&handle), SessionAddress::session(id), ui);
                }
                Err(err) => {
                    tracing::warn!(%err, "新建会话失败");
                    error.set(Some(format!("新建会话失败: {err}")));
                }
            }
        });
    };

    let on_select = move |id: String| {
        ui.active.set(Some(id.clone()));
        ui.active_address
            .set(Some(SessionAddress::session(id.clone())));
        ui.lineage.set(Vec::new());
        ui.subagent_expanded.set(Vec::new());
        ui.subagent_error.set(None);
        ui.subagent_loading.set(false);
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

    let on_send = move |request: SendRequest| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(address) = (ui.active_address)() else {
            return;
        };

        let mut echo = request.text.clone();
        if !request.attachments.is_empty() {
            let names = request
                .attachments
                .iter()
                .filter(|item| !item.is_image())
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            if !names.is_empty() {
                if echo.is_empty() {
                    echo = format!("[附件] {names}");
                } else {
                    echo.push_str(&format!("\n[附件] {names}"));
                }
            }
            for attachment in request.attachments.iter().filter(|item| item.is_image()) {
                let data = base64::engine::general_purpose::STANDARD.encode(&attachment.bytes);
                echo.push_str(&format!(
                    "\n\n![{}](data:{};base64,{data})",
                    attachment.name, attachment.media_type
                ));
            }
        }
        let local_id = format!("local-{}", ui.messages.read().len());
        ui.messages
            .write()
            .push(MessageRow::new(local_id, dsh_ui::Role::User, echo));

        let mode = match (ui.send_mode)() {
            SendMode::Queue => dsh_proto::session::PromptMode::Queue,
            SendMode::Steer => dsh_proto::session::PromptMode::Steer,
        };
        let mut error = ui.error;
        spawn(async move {
            // 纯斜杠命令交给 Host 的 commands/execute，不进入模型。
            if request.attachments.is_empty()
                && request.text.trim_start().starts_with('/')
                && !address.is_subagent()
            {
                match handle
                    .execute_command(
                        address.session_id().as_str().to_string(),
                        request.text.trim(),
                    )
                    .await
                {
                    Ok(Some(execution)) => {
                        if let dsh_proto::interaction::CommandResult::Error { text } =
                            execution.result
                        {
                            error.set(Some(text));
                        }
                    }
                    Ok(None) => {
                        error.set(Some("未知的斜杠命令".to_string()));
                    }
                    Err(err) => error.set(Some(format!("命令执行失败: {err}"))),
                }
                return;
            }

            let mut content = Vec::new();
            if !request.text.trim().is_empty() {
                content.push(PromptContent::text(request.text));
            }
            for attachment in request.attachments {
                if attachment.is_image() {
                    let data = base64::engine::general_purpose::STANDARD.encode(&attachment.bytes);
                    content.push(PromptContent::Image {
                        media_type: attachment.media_type,
                        data,
                        name: Some(attachment.name),
                    });
                } else {
                    if address.is_subagent() {
                        error.set(Some("子会话不支持文件附件".to_string()));
                        return;
                    }
                    match handle
                        .upload_file(
                            address.session_id().as_str().to_string(),
                            attachment.name,
                            attachment.media_type,
                            attachment.bytes,
                        )
                        .await
                    {
                        Ok(upload) => content.push(PromptContent::File {
                            receipt_id: upload.receipt_id,
                        }),
                        Err(err) => {
                            error.set(Some(format!("附件上传失败: {err}")));
                            return;
                        }
                    }
                }
            }
            if content.is_empty() {
                return;
            }

            let result = match &address {
                SessionAddress::Subagent {
                    parent_session_id,
                    child_session_id,
                    mode: sub_mode,
                } if sub_mode.is_continuable() => handle
                    .prompt_subagent_content(
                        parent_session_id.as_str(),
                        child_session_id.as_str(),
                        *sub_mode,
                        content,
                        mode,
                    )
                    .await
                    .map(|_| ()),
                _ => {
                    handle
                        .prompt_content(address.session_id().as_str().to_string(), content, mode)
                        .await
                }
            };
            if let Err(err) = result {
                tracing::warn!(%err, "发送失败");
                error.set(Some(format!("发送失败: {err}")));
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
        let mut error = ui.error;
        spawn(async move {
            if let Err(err) = handle.answer_approval(client_id, event_id, allow).await {
                tracing::warn!(%err, "审批回执失败");
                error.set(Some(format!("审批回执失败: {err}")));
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
        let mut error = ui.error;
        spawn(async move {
            if let Err(err) = handle.answer_question(agent_id, call_id, payload).await {
                tracing::warn!(%err, "回答提交失败");
                error.set(Some(format!("回答提交失败: {err}")));
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
        ui.subagent_error.set(None);
        ui.subagent_loading.set(false);
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
        ui.subagent_error.set(None);
        ui.subagent_loading.set(false);
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

    let on_copy = move |text: String| {
        let escaped = serde_json::to_string(&text).unwrap_or_else(|_| "\"\"".to_string());
        let _ = document::eval(&format!(
            r#"const text = {escaped};
if (navigator.clipboard && navigator.clipboard.writeText) {{
  navigator.clipboard.writeText(text).catch(() => {{}});
}} else {{
  const area = document.createElement('textarea');
  area.value = text;
  document.body.appendChild(area);
  area.select();
  document.execCommand('copy');
  area.remove();
}}"#
        ));
    };

    let on_feedback = move |(message_id, positive): (String, bool)| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(session_id) = (ui.active)() else {
            return;
        };
        let mut error = ui.error;
        spawn(async move {
            let request = MessageFeedbackPutRequest {
                session_id,
                message_id,
                rating: if positive { "positive" } else { "negative" }.to_string(),
                note: None,
                category: None,
                if_version: serde_json::Value::Null,
            };
            if let Err(err) = handle.put_feedback(request).await {
                error.set(Some(format!("反馈提交失败: {err}")));
            }
        });
    };

    let on_retry = move |message_id: String| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(address) = (ui.active_address)() else {
            return;
        };
        let messages = (ui.messages)();
        let Some(index) = messages.iter().position(|item| item.id == message_id) else {
            return;
        };
        let Some(user) = messages[..index]
            .iter()
            .rev()
            .find(|item| item.role == dsh_ui::Role::User)
            .map(|item| item.text.clone())
        else {
            ui.error.set(Some("找不到可重试的用户消息".to_string()));
            return;
        };
        let mut error = ui.error;
        spawn(async move {
            let mode = dsh_proto::session::PromptMode::Queue;
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
                        user,
                        mode,
                    )
                    .await
                    .map(|_| ()),
                _ => {
                    handle
                        .prompt_with_mode(address.session_id().as_str().to_string(), user, mode)
                        .await
                }
            };
            if let Err(err) = result {
                error.set(Some(format!("重试失败: {err}")));
            }
        });
    };

    let on_search = move |query: String| {
        ui.search_query.set(query.clone());
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let mut results = ui.search_results;
        let mut searching = ui.searching;
        let mut error = ui.error;
        let current = ui.search_query;
        if query.trim().is_empty() {
            results.set(Vec::new());
            searching.set(false);
            return;
        }
        searching.set(true);
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(180)).await;
            if (current)() != query {
                return;
            }
            match handle.search_sessions(query.clone()).await {
                Ok(value) => {
                    if (current)() == query {
                        results.set(map::search_hits(&value));
                    }
                }
                Err(err) => {
                    tracing::warn!(%err, "会话搜索失败");
                    if (current)() == query {
                        error.set(Some(format!("搜索失败: {err}")));
                    }
                }
            }
            if (current)() == query {
                searching.set(false);
            }
        });
    };

    let on_search_clear = move |_| {
        ui.search_query.set(String::new());
        ui.search_results.set(Vec::new());
        ui.searching.set(false);
    };

    let on_pin = move |(session_id, pinned): (String, bool)| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let mut pinned_ids = ui.pinned;
        let mut error = ui.error;
        spawn(async move {
            match handle.pin_session(session_id, pinned).await {
                Ok(value) => pinned_ids.set(value.pinned_session_ids),
                Err(err) => error.set(Some(format!("置顶失败: {err}"))),
            }
        });
    };

    let on_fork = move |session_id: String| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let mut sessions = ui.all_sessions;
        let mut active = ui.active;
        let mut address = ui.active_address;
        let mut lineage = ui.lineage;
        let mut error = ui.error;
        spawn(async move {
            match handle.fork_session(session_id, None).await {
                Ok(value) => {
                    let child = value.session_id.as_str().to_string();
                    if let Ok(list) = handle.list_sessions().await {
                        sessions.set(list.iter().map(map::to_summary).collect());
                    }
                    active.set(Some(child.clone()));
                    address.set(Some(SessionAddress::session(child.clone())));
                    lineage.set(Vec::new());
                    spawn_follow(Arc::clone(&handle), SessionAddress::session(child), ui);
                }
                Err(err) => error.set(Some(format!("Fork 失败: {err}"))),
            }
        });
    };

    let on_new_workspace = move |path: String| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let mut workspaces = ui.workspaces;
        let mut error = ui.error;
        spawn(async move {
            match handle.create_workspace(path).await {
                Ok(value) => {
                    let mut list = workspaces.read().clone();
                    if let Some(existing) = list
                        .iter_mut()
                        .find(|item| item.workspace_id == value.workspace.workspace_id)
                    {
                        *existing = value.workspace;
                    } else {
                        list.push(value.workspace);
                    }
                    workspaces.set(list);
                }
                Err(err) => error.set(Some(format!("创建工作区失败: {err}"))),
            }
        });
    };

    let on_rename_workspace = move |(workspace_id, title): (String, String)| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let mut workspaces = ui.workspaces;
        let mut error = ui.error;
        spawn(async move {
            match handle.rename_workspace(workspace_id, title).await {
                Ok(value) => {
                    let mut list = workspaces.read().clone();
                    if let Some(item) = list
                        .iter_mut()
                        .find(|item| item.workspace_id == value.workspace.workspace_id)
                    {
                        *item = value.workspace;
                    }
                    workspaces.set(list);
                }
                Err(err) => error.set(Some(format!("重命名工作区失败: {err}"))),
            }
        });
    };

    let on_move_workspace = move |(workspace_id, up): (String, bool)| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let list = (ui.workspaces)();
        let Some(pos) = list
            .iter()
            .position(|item| item.workspace_id == workspace_id)
        else {
            return;
        };
        let before = if up {
            pos.checked_sub(1).and_then(|index| list.get(index))
        } else {
            list.get(pos + 2)
        }
        .map(|item| item.workspace_id.clone());
        let mut workspaces = ui.workspaces;
        let mut error = ui.error;
        spawn(async move {
            match handle.move_workspace(workspace_id, before).await {
                Ok(order) => {
                    let mut current = workspaces.read().clone();
                    current.sort_by_key(|item| {
                        order
                            .iter()
                            .position(|id| id == &item.workspace_id)
                            .unwrap_or(usize::MAX)
                    });
                    workspaces.set(current);
                }
                Err(err) => error.set(Some(format!("调整工作区顺序失败: {err}"))),
            }
        });
    };

    let on_delete_workspace = move |workspace_id: String| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let mut workspaces = ui.workspaces;
        let mut error = ui.error;
        spawn(async move {
            match handle.delete_workspace(workspace_id.clone()).await {
                Ok(()) => {
                    workspaces
                        .write()
                        .retain(|item| item.workspace_id != workspace_id);
                }
                Err(err) => error.set(Some(format!("删除工作区失败: {err}"))),
            }
        });
    };

    let on_settings_change = move |change: SettingsChange| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(snapshot) = (ui.settings)() else {
            return;
        };
        // 字号/字体先落到 CSS 变量，Host 写入返回后再以设置快照为准。
        match &change {
            SettingsChange::FontSize(size) => {
                let _ = document::eval(&format!(
                    "document.body.style.setProperty('--dsh-content-font-size', '{size}px')"
                ));
            }
            SettingsChange::CodeFontSize(size) => {
                let _ = document::eval(&format!(
                    "document.body.style.setProperty('--dsh-code-font-size', '{size}px')"
                ));
            }
            SettingsChange::TextFontFamily(value) => {
                let escaped = escape_js_string(value);
                let _ = document::eval(&format!(
                    "if ('{escaped}') document.body.style.setProperty('--dsh-font-family-text', '{escaped}'); else document.body.style.removeProperty('--dsh-font-family-text')"
                ));
            }
            SettingsChange::CodeFontFamily(value) => {
                let escaped = escape_js_string(value);
                let _ = document::eval(&format!(
                    "if ('{escaped}') document.body.style.setProperty('--dsh-font-family-code', '{escaped}'); else document.body.style.removeProperty('--dsh-font-family-code')"
                ));
            }
            _ => {}
        }
        let (ns, patch, revision) = match &change {
            SettingsChange::Theme(theme) => {
                ui.theme.set(*theme);
                (
                    THEME_NS,
                    serde_json::json!({ "preference": theme.attribute() }),
                    snapshot.namespace(THEME_NS).map(|item| item.revision),
                )
            }
            SettingsChange::Locale(locale) => {
                ui.locale.set(*locale);
                (
                    LOCALE_NS,
                    serde_json::json!({ "preference": locale.code() }),
                    snapshot.namespace(LOCALE_NS).map(|item| item.revision),
                )
            }
            SettingsChange::FontSize(size) => (
                THEME_NS,
                serde_json::json!({ "fontSize": size }),
                snapshot.namespace(THEME_NS).map(|item| item.revision),
            ),
            SettingsChange::CodeFontSize(size) => (
                THEME_NS,
                serde_json::json!({ "codeFontSize": size }),
                snapshot.namespace(THEME_NS).map(|item| item.revision),
            ),
            SettingsChange::TextFontFamily(value) => (
                THEME_NS,
                serde_json::json!({ "textFontFamily": value }),
                snapshot.namespace(THEME_NS).map(|item| item.revision),
            ),
            SettingsChange::CodeFontFamily(value) => (
                THEME_NS,
                serde_json::json!({ "codeFontFamily": value }),
                snapshot.namespace(THEME_NS).map(|item| item.revision),
            ),
            SettingsChange::MaxDepth(value) => (
                SUBAGENT_NS,
                serde_json::json!({ "maxDepth": value }),
                snapshot.namespace(SUBAGENT_NS).map(|item| item.revision),
            ),
            SettingsChange::MaxActiveSubagents(value) => (
                SUBAGENT_NS,
                serde_json::json!({ "maxActiveSubagents": value }),
                snapshot.namespace(SUBAGENT_NS).map(|item| item.revision),
            ),
        };
        ui.settings_saving.set(true);
        let mut settings = ui.settings;
        let mut saving = ui.settings_saving;
        let mut error = ui.error;
        spawn(async move {
            match handle.update_settings(ns, patch, revision).await {
                Ok(view) => {
                    if let Some(current) = settings.write().as_mut() {
                        if let Some(slot) = current
                            .namespaces
                            .iter_mut()
                            .find(|item| item.ns == view.ns)
                        {
                            *slot = view;
                        } else {
                            current.namespaces.push(view);
                        }
                    }
                    apply_appearance(&(ui.theme)(), &(ui.settings)());
                }
                Err(err) => error.set(Some(format!("设置保存失败: {err}"))),
            }
            saving.set(false);
        });
    };

    let on_select_model = move |(provider, model): (String, String)| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(session_id) = (ui.active)() else {
            return;
        };
        let mut selected = ui.selected_model;
        let mut error = ui.error;
        spawn(async move {
            match handle.select_model(session_id, provider, model, None).await {
                Ok(value) => selected.set(Some(value.selected)),
                Err(err) => error.set(Some(format!("切换模型失败: {err}"))),
            }
        });
    };

    let on_select_permission = move |value: String| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(session_id) = (ui.active)() else {
            return;
        };
        let mut selected = ui.selected_permission;
        let mut error = ui.error;
        spawn(async move {
            match handle
                .execute_command(session_id, format!("/permission {value}"))
                .await
            {
                Ok(_) => selected.set(Some(value)),
                Err(err) => error.set(Some(format!("切换权限失败: {err}"))),
            }
        });
    };

    let on_apply_preset = move |preset: String| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(session_id) = (ui.active)() else {
            return;
        };
        let mut default_preset = ui.default_preset;
        let mut error = ui.error;
        spawn(async move {
            match handle.select_agent_preset(session_id, preset.clone()).await {
                Ok(()) => default_preset.set(Some(preset)),
                Err(err) => error.set(Some(format!("应用 Agent preset 失败: {err}"))),
            }
        });
    };

    let on_file_query = move |query: String| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(session_id) = (ui.active)() else {
            return;
        };
        let mut refs = ui.file_refs;
        spawn(async move {
            if let Ok(items) = handle.list_file_references(session_id, query).await {
                refs.set(items);
            }
        });
    };

    let on_dismiss_error = move |_| ui.error.set(None);
    let on_command_query = move |_| {};

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

    let on_file_toggle = move |path: String| {
        let mut expanded = (ui.file_expanded)();
        if let Some(pos) = expanded.iter().position(|item| item == &path) {
            expanded.remove(pos);
            ui.file_expanded.set(expanded);
            return;
        }
        expanded.push(path.clone());
        ui.file_expanded.set(expanded);
        if (ui.file_children)().contains_key(&path) {
            return;
        }
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(address) = (ui.active_address)() else {
            return;
        };
        let scope = address.session_id().as_str().to_string();
        let mut children = ui.file_children;
        let mut loading = ui.file_loading;
        loading.set(true);
        spawn(async move {
            match handle.list_files(&scope, &path).await {
                Ok(entries) => {
                    let items: Vec<FileEntry> = entries
                        .iter()
                        .map(|item| FileEntry {
                            path: item.path.clone(),
                            name: item.name.clone(),
                            is_dir: item.is_dir,
                        })
                        .collect();
                    children.write().insert(path, items);
                }
                Err(err) => tracing::warn!(%err, "目录加载失败"),
            }
            loading.set(false);
        });
    };

    let on_file_refresh = move |_| {
        let Some(handle) = ui.client.read().clone() else {
            return;
        };
        let Some(address) = (ui.active_address)() else {
            return;
        };
        let scope = address.session_id().as_str().to_string();
        let root = {
            let root = (ui.file_root)();
            if root.is_empty() {
                std::env::current_dir()
                    .ok()
                    .map(|path| path.to_string_lossy().to_string())
                    .unwrap_or_else(|| ".".to_string())
            } else {
                root
            }
        };
        let mut files = ui.files;
        let mut children = ui.file_children;
        let mut loading = ui.file_loading;
        loading.set(true);
        spawn(async move {
            match handle.list_files(&scope, &root).await {
                Ok(entries) => {
                    let items: Vec<FileEntry> = entries
                        .iter()
                        .map(|item| FileEntry {
                            path: item.path.clone(),
                            name: item.name.clone(),
                            is_dir: item.is_dir,
                        })
                        .collect();
                    files.set(items.clone());
                    children.write().insert(root, items);
                }
                Err(err) => tracing::warn!(%err, "文件树刷新失败"),
            }
            loading.set(false);
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

    let settings_value = (ui.settings)();
    let settings_view = settings_value
        .as_ref()
        .map(|value| map::settings_snapshot(value, (ui.theme)(), (ui.locale)()));
    let model_catalog = (ui.models)();
    let model_groups = model_catalog
        .as_ref()
        .map(map::model_groups)
        .unwrap_or_default();
    let selected_model_pair = (ui.selected_model)()
        .map(|value| (value.provider, value.model))
        .or_else(|| {
            model_catalog.as_ref().map(|catalog| {
                (
                    catalog.default.provider.clone(),
                    catalog.default.model.clone(),
                )
            })
        });
    let permissions_view = (ui.permissions)()
        .as_ref()
        .map(map::permissions)
        .unwrap_or_default();
    let selected_permission = (ui.selected_permission)().or_else(|| {
        (ui.active)().and_then(|id| {
            projection_store
                .get(&id, "permissions")
                .and_then(|value| value.get("currentValue"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
    });
    let agent_presets_view = (ui.agent_presets)()
        .as_ref()
        .map(map::agent_presets)
        .unwrap_or_default();
    let workspaces_view = map::workspace_tabs(&(ui.workspaces)());
    let commands_view = map::commands(&(ui.commands)());
    let file_refs_view = map::file_refs(&(ui.file_refs)());
    let plan_state = (ui.active)()
        .map(|id| map::plan_state(&projection_store, &id))
        .unwrap_or_default();
    let subagent_migration = (ui.active)()
        .map(|id| projection_store.migration_required(&id))
        .unwrap_or(false);

    rsx! {
        document::Style { "{APP_CSS}" }
        AppShell {
            sessions: {
                let archived = (ui.archived)();
                let workspaces = (ui.workspaces)();
                let active = (ui.active)();
                let waiting = !(ui.approvals)().is_empty();
                let errored = (ui.error)().is_some();
                (ui.all_sessions)()
                    .into_iter()
                    .filter(|item| !item.is_subagent && !archived.contains(&item.id))
                    .map(|mut item| {
                        if let Some(workspace) = workspaces
                            .iter()
                            .find(|workspace| workspace.session_ids.iter().any(|id| id == &item.id))
                        {
                            item.workspace = workspace.title.clone();
                        }
                        if active.as_deref() == Some(item.id.as_str()) {
                            if errored {
                                item.status = dsh_ui::SessionStatus::Error;
                            } else if waiting {
                                item.status = dsh_ui::SessionStatus::Waiting;
                            }
                        }
                        item
                    })
                    .collect::<Vec<_>>()
            },
            active_id: (ui.active)(),
            active_title: (ui.active_title)(),
            messages: (ui.messages)(),
            trajectory: (ui.trajectory)(),
            files: (ui.files)(),
            file_children: (ui.file_children)(),
            file_expanded: (ui.file_expanded)(),
            file_loading: (ui.file_loading)(),
            on_file_toggle,
            on_file_refresh,
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
            settings: settings_view,
            model_groups,
            permissions: permissions_view,
            agent_presets: agent_presets_view,
            selected_model: selected_model_pair,
            selected_permission,
            settings_saving: (ui.settings_saving)(),
            on_settings_change,
            on_select_model,
            on_select_permission,
            on_apply_preset,
            commands: commands_view,
            file_refs: file_refs_view,
            plan: plan_state,
            on_command_query,
            on_file_query,
            on_copy,
            on_feedback,
            on_retry,
            workspaces: workspaces_view,
            pinned: (ui.pinned)(),
            search_query: (ui.search_query)(),
            search_results: (ui.search_results)(),
            searching: (ui.searching)(),
            on_search,
            on_search_clear,
            on_pin,
            on_fork,
            on_new_workspace,
            on_rename_workspace,
            on_delete_workspace,
            on_move_workspace,
            error: (ui.error)(),
            on_dismiss_error,
            is_subagent,
            subagent_mode,
            parent_available,
            lineage,
            subagent_rows,
            subagent_tree: (ui.subagent_tree)(),
            subagent_expanded: (ui.subagent_expanded)(),
            subagent_error: (ui.subagent_error)(),
            subagent_loading: (ui.subagent_loading)(),
            subagent_migration,
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
    ui.subagent_error.set(None);
    ui.subagent_loading.set(false);
    ui.subagent_error.set(None);
    ui.subagent_loading.set(false);
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

    // 工作区/归档/置顶来自 workspace/follow：首帧做基线，后续帧持续更新。
    if let Ok(mut workspace) = handle.follow_workspace() {
        if let Ok(Some(StreamEvent::Frame(value))) =
            tokio::time::timeout(Duration::from_secs(5), workspace.next()).await
            && let Ok(frame) = serde_json::from_value::<WorkspaceFollowFrame>(value)
        {
            apply_workspace_frame(&frame, ui.workspaces, ui.archived, ui.pinned);
            tracing::info!(count = (ui.archived)().len(), "workspace baseline 已应用");
        }
        let workspaces = ui.workspaces;
        let archived = ui.archived;
        let pinned = ui.pinned;
        spawn(async move {
            while let Some(event) = workspace.next().await {
                let StreamEvent::Frame(value) = event else {
                    break;
                };
                if let Ok(frame) = serde_json::from_value::<WorkspaceFollowFrame>(value) {
                    apply_workspace_frame(&frame, workspaces, archived, pinned);
                }
            }
        });
    }

    // 设置、模型、权限与 preset 目录：失败不阻塞主流程。
    if let Ok(describe) = handle.describe_settings().await {
        let snapshot = map::settings_snapshot(&describe, (ui.theme)(), (ui.locale)());
        ui.theme.set(snapshot.theme);
        ui.locale.set(snapshot.locale);
        ui.settings.set(Some(describe));
    }
    if let Ok(catalog) = handle.model_catalog().await {
        if (ui.selected_model)().is_none() {
            ui.selected_model.set(Some(catalog.default.clone()));
        }
        ui.models.set(Some(catalog));
    }
    if let Ok(catalog) = handle.permission_catalog().await {
        if (ui.selected_permission)().is_none() {
            ui.selected_permission
                .set(Some(catalog.default_preset.clone()));
        }
        ui.permissions.set(Some(catalog));
    }
    if let Ok(roster) = handle.agent_presets().await {
        ui.default_preset.set(
            roster
                .presets
                .iter()
                .find(|item| item.is_default)
                .map(|item| item.id.clone()),
        );
        ui.agent_presets.set(Some(roster));
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

    if std::env::var("DSH_FAKE_RICH").is_ok() {
        let mut messages = ui.messages;
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            let plan = serde_json::json!({
                "plan": "# 迁移计划\n\n1. 抽出 protocol\n2. 接上 settings\n3. 补测试"
            });
            let mut plan_row = MessageRow::new(
                "fake-plan",
                dsh_ui::Role::Tool,
                format!("exit_plan_mode {plan}"),
            );
            plan_row = plan_row.tool("exit_plan_mode");
            messages.set(vec![
                MessageRow::new(
                    "fake-u1",
                    dsh_ui::Role::User,
                    "解释 src/main.rs:24-30 的改动，并给出公式 $E=mc^2$。",
                ),
                MessageRow::new(
                    "fake-a1",
                    dsh_ui::Role::Assistant,
                    "改动集中在 `src/main.rs:24-30`。\n\n行内公式 $E=mc^2$，块级：\n\n$$\\frac{a+b}{2} \\le \\sqrt{x^2+y^2}$$",
                ),
                plan_row,
            ]);
        });
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
    let mut commands = ui.commands;
    let mut file_refs = ui.file_refs;
    let mut files = ui.files;
    let mut file_children = ui.file_children;
    let mut file_expanded = ui.file_expanded;
    let mut file_loading = ui.file_loading;
    let mut file_root = ui.file_root;
    let active = ui.active;

    let catalog_ui = ui;
    file_refs.set(Vec::new());
    if is_subagent {
        commands.set(Vec::new());
    } else {
        let command_handle = Arc::clone(&handle);
        let command_session = session_id.clone();
        spawn(async move {
            if let Ok(items) = command_handle.list_commands(command_session).await {
                commands.set(items);
            }
        });
    }

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
                        if is_subagent {
                            files.set(Vec::new());
                            file_children.set(HashMap::new());
                            file_expanded.set(Vec::new());
                        } else {
                            let root = view
                                .header
                                .as_ref()
                                .and_then(|header| header.cwd.clone())
                                .or_else(|| {
                                    std::env::current_dir()
                                        .ok()
                                        .map(|path| path.to_string_lossy().to_string())
                                })
                                .unwrap_or_else(|| ".".to_string());
                            file_root.set(root.clone());
                            let file_handle = Arc::clone(&handle);
                            let scope = session_id.clone();
                            spawn(async move {
                                file_loading.set(true);
                                match file_handle.list_files(&scope, &root).await {
                                    Ok(entries) => {
                                        let items: Vec<FileEntry> = entries
                                            .iter()
                                            .map(|item| FileEntry {
                                                path: item.path.clone(),
                                                name: item.name.clone(),
                                                is_dir: item.is_dir,
                                            })
                                            .collect();
                                        files.set(items.clone());
                                        let mut map = file_children.read().clone();
                                        map.insert(root, items);
                                        file_children.set(map);
                                        file_expanded.set(Vec::new());
                                    }
                                    Err(err) => tracing::warn!(%err, "文件树加载失败"),
                                }
                                file_loading.set(false);
                            });
                        }
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
                        } else if !is_subagent && std::env::var("DSH_FAKE_SUBAGENT").is_err() {
                            // 旧 Host 或缺 projection：主动拉一次，失败时 header 仍显示入口 + 重试。
                            refresh_catalog(Arc::clone(&handle), session_id.clone(), catalog_ui);
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
    let mut error = ui.subagent_error;
    let mut loading = ui.subagent_loading;

    loading.set(true);
    error.set(None);
    spawn(async move {
        match handle.refresh_projections(&parent_id).await {
            Ok(value) => {
                projections.write().seed_result(&parent_id, &value);
                let store = projections.read().clone();
                if let Some(catalog) = store.get(&parent_id, "subagentCatalog") {
                    let rows = map::catalog_rows(&parent_id, catalog, &store);
                    tree.write().insert(parent_id.clone(), rows);
                }
                error.set(None);
            }
            Err(err) => {
                tracing::warn!(%err, %parent_id, "子 Agent 目录加载失败");
                error.set(Some(format!("子 Agent 目录读取失败: {err}")));
            }
        }
        loading.set(false);
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

/// 应用 `ui-theme` 的主题、字号与字体族（Host 设置 + 本地即时状态共用）。
fn apply_appearance(theme: &Theme, settings: &Option<SettingsDescribeValue>) {
    let attr = theme.attribute();
    let theme_ns = settings
        .as_ref()
        .and_then(|value| value.namespace(THEME_NS));
    let font_size = theme_ns
        .and_then(|item| item.u64_field("fontSize"))
        .unwrap_or(14);
    let code_size = theme_ns
        .and_then(|item| item.u64_field("codeFontSize"))
        .unwrap_or(12);
    let text_family = theme_ns
        .and_then(|item| item.str_field("textFontFamily"))
        .unwrap_or_default();
    let code_family = theme_ns
        .and_then(|item| item.str_field("codeFontFamily"))
        .unwrap_or_default();
    let text_family = escape_js_string(text_family);
    let code_family = escape_js_string(code_family);
    let _ = document::eval(&format!(
        r#"const theme = '{attr}';
const dark = theme === 'dark' || (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
if (dark) document.body.setAttribute('data-ds-dark-theme', '');
else document.body.removeAttribute('data-ds-dark-theme');
document.documentElement.style.colorScheme = dark ? 'dark' : 'light';
document.body.style.setProperty('--dsh-content-font-size', '{font_size}px');
document.body.style.setProperty('--dsh-code-font-size', '{code_size}px');
const textFamily = '{text_family}';
const codeFamily = '{code_family}';
if (textFamily) document.body.style.setProperty('--dsh-font-family-text', textFamily);
else document.body.style.removeProperty('--dsh-font-family-text');
if (codeFamily) document.body.style.setProperty('--dsh-font-family-code', codeFamily);
else document.body.style.removeProperty('--dsh-font-family-code');
try {{ localStorage.setItem('dsh-theme', theme === 'system' ? '' : theme); }} catch (_error) {{}}
document.documentElement.removeAttribute('data-ds-boot-dark');"#
    ));
}

fn escape_js_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace(['\n', '\r'], " ")
}

/// 把 `workspace/follow` 的一帧写入侧栏状态。
fn apply_workspace_frame(
    frame: &WorkspaceFollowFrame,
    mut workspaces: Signal<Vec<WorkspaceView>>,
    mut archived: Signal<Vec<String>>,
    mut pinned: Signal<Vec<String>>,
) {
    let apply_baseline = |value: &dsh_proto::workspace::WorkspaceBaseline,
                          workspaces: &mut Signal<Vec<WorkspaceView>>,
                          archived: &mut Signal<Vec<String>>,
                          pinned: &mut Signal<Vec<String>>| {
        workspaces.set(value.items.clone());
        archived.set(value.archived_session_ids.clone());
        pinned.set(value.pinned_session_ids.clone());
    };
    match frame {
        WorkspaceFollowFrame::Baseline { value } | WorkspaceFollowFrame::Replace { value } => {
            apply_baseline(value, &mut workspaces, &mut archived, &mut pinned);
        }
        WorkspaceFollowFrame::Upsert { workspace } => {
            let mut items = workspaces.read().clone();
            if let Some(slot) = items
                .iter_mut()
                .find(|item| item.workspace_id == workspace.workspace_id)
            {
                *slot = workspace.clone();
            } else {
                items.push(workspace.clone());
            }
            workspaces.set(items);
        }
        WorkspaceFollowFrame::Remove { workspace_id } => {
            workspaces
                .write()
                .retain(|item| &item.workspace_id != workspace_id);
        }
        WorkspaceFollowFrame::Order { workspace_ids } => {
            let mut items = workspaces.read().clone();
            items.sort_by_key(|item| {
                workspace_ids
                    .iter()
                    .position(|id| id == &item.workspace_id)
                    .unwrap_or(usize::MAX)
            });
            workspaces.set(items);
        }
        WorkspaceFollowFrame::Archived {
            archived_session_ids,
        } => archived.set(archived_session_ids.clone()),
        WorkspaceFollowFrame::Pinned { pinned_session_ids } => {
            pinned.set(pinned_session_ids.clone())
        }
    }
}
