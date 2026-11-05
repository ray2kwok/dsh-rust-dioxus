//! 把协议类型映射成 UI 需要的视图类型。
//!
//! 刻意不做「精确建模」：`SessionWireEvent.data` 的形状由事件类型决定，
//! 而具体消息形状要等真实 prompt 事件才能确认。这里按
//! 「已知字段优先、未知结构降级」的顺序提取，保证任何事件都能渲染出东西。

use std::collections::HashMap;

use dsh_proto::interaction::{
    AgentPresetRoster, CommandDescriptor, FileReferenceCandidate, ModelCatalog, PermissionCatalog,
};
use dsh_proto::session::{
    SessionSearchValue, SessionSummary as WireSummary, SessionWireEvent, SubagentMode as WireMode,
};
use dsh_proto::settings::{LOCALE_NS, SUBAGENT_NS, SettingsDescribeValue, THEME_NS};
use dsh_proto::subagent::{SubagentCatalogEntry, SubagentTiming, TokenUsage};
use dsh_proto::workspace::WorkspaceView;
use dsh_state::{ProjectionStore, SessionView};
use dsh_ui::{
    AgentPresetItemView, ApprovalItem, CommandItemView, FileRefItemView, JobRow, MessageRow,
    ModelGroupView, ModelItemView, PermissionItemView, PlanState, QuestionCard, QuestionItemView,
    QueueItem, Role, SearchHit, SessionStatus, SessionSummary, SubagentMode, SubagentRow,
    SubagentState, TrajectoryRow, WorkspaceTab,
};
use dsh_ui::{Locale, SettingsSnapshot, Theme};
use serde_json::Value;

/// 会话摘要 → 列表项。
pub fn to_summary(wire: &WireSummary) -> SessionSummary {
    // Host 的列表里没有标题（标题来自 projections/rename）；先用短 id 占位，
    // 避免与左侧的「工作区名」重复。
    // 标题来自 projections/rename，暂用短 id 占位；不加语言前缀，避免与 i18n 耦合
    let title = short_id(wire.session_id.as_str());

    // M0 只有「运行中/空闲」两态；等待审批需要 api-session/status 事件，M2 接入
    let status = if wire.running {
        SessionStatus::Running
    } else {
        SessionStatus::Idle
    };

    SessionSummary {
        id: wire.session_id.as_str().to_string(),
        title,
        // 用目录名而不是完整路径，避免侧栏被长路径撑开
        workspace: wire
            .cwd
            .as_deref()
            .and_then(|cwd| cwd.rsplit('/').find(|seg| !seg.is_empty()))
            .map(str::to_string)
            .unwrap_or_else(|| "未分组".to_string()),
        status,
        updated: format_time(wire.updated_at),
        parent_id: wire
            .parent_session_id
            .as_ref()
            .map(|id| id.as_str().to_string()),
        is_subagent: wire.origin.as_deref() == Some("subagent"),
    }
}

/// 子 Agent 模式（wire → 视图）。
pub fn ui_mode(mode: WireMode) -> SubagentMode {
    match mode {
        WireMode::OneShot => SubagentMode::OneShot,
        WireMode::Continuable => SubagentMode::Continuable,
        WireMode::External => SubagentMode::External,
        WireMode::Unknown => SubagentMode::Unknown,
    }
}

/// 子 Agent 模式（视图 → wire）。
pub fn wire_mode(mode: SubagentMode) -> WireMode {
    match mode {
        SubagentMode::OneShot => WireMode::OneShot,
        SubagentMode::Continuable => WireMode::Continuable,
        SubagentMode::External => WireMode::External,
        SubagentMode::Unknown => WireMode::Unknown,
    }
}

/// `subagentCatalog` + 子会话的 timing/tokenUsage → 目录行。
///
/// 目录成员来自父会话（`parent_id`），每行的实时数据来自子会话自己的
/// `subagentTiming` / `tokenUsage`，两者都在 [`ProjectionStore`] 里。
pub fn catalog_rows(parent_id: &str, catalog: &Value, store: &ProjectionStore) -> Vec<SubagentRow> {
    let Ok(entries) = serde_json::from_value::<Vec<SubagentCatalogEntry>>(catalog.clone()) else {
        return Vec::new();
    };

    entries
        .into_iter()
        .map(|entry| {
            let id = entry.id.as_str().to_string();
            let timing = store
                .get(&id, "subagentTiming")
                .and_then(|value| serde_json::from_value::<SubagentTiming>(value.clone()).ok())
                .unwrap_or_default();
            let tokens = store
                .get(&id, "tokenUsage")
                .and_then(|value| serde_json::from_value::<TokenUsage>(value.clone()).ok())
                .map(|usage| usage.total())
                .unwrap_or(0);

            let state = if timing.is_running() {
                SubagentState::Running
            } else if timing.last_turn_completed == Some(true) {
                SubagentState::Completed
            } else {
                SubagentState::Idle
            };

            SubagentRow {
                id: id.clone(),
                parent_id: parent_id.to_string(),
                mode: ui_mode(entry.mode),
                label: entry.label.unwrap_or(id),
                state,
                elapsed_ms: timing.elapsed_ms(),
                tokens,
            }
        })
        .collect()
}

/// 子会话标题：优先 descriptor 的 label，其次 `title` projection。
pub fn subagent_title(view: &SessionView) -> Option<String> {
    view.projection("subagent")
        .and_then(|value| value.get("label"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_string)
        .or_else(|| title(view))
}

/// `approval/request` waterfall → 审批卡片。
///
/// `request` 的具体形状由发起方决定（工具名、参数、理由…），这里只做
/// 「尽量取名字、其余原样展示」，不猜测结构。
pub fn approval(event_id: String, agent_id: String, request: &Value) -> ApprovalItem {
    let tool = request
        .get("tool")
        .or_else(|| request.get("toolName"))
        .and_then(Value::as_str)
        .unwrap_or("工具");

    ApprovalItem {
        event_id,
        agent_id,
        title: format!("{tool} 请求执行"),
        detail: truncate(&request.to_string(), 400),
    }
}

/// 会话视图 → 消息行；末尾追加正在流式输出的行。
///
/// `tool/call` 与 `tool/result` 按 `callId` 合成一张卡片：调用在前、输出追加在后，
/// 避免同一个工具调用出现两张卡、结果卡还丢名字。
pub fn rows(view: &SessionView) -> Vec<MessageRow> {
    let mut rows: Vec<MessageRow> = Vec::new();
    let mut calls: HashMap<String, usize> = HashMap::new();

    for event in &view.entries {
        if event.kind == "tool/result" {
            // 线上形状：`message.toolCallId`；持久化后的记录会提到顶层，两边都认。
            let call_id = event
                .data
                .get("toolCallId")
                .or_else(|| {
                    event
                        .data
                        .get("message")
                        .and_then(|message| message.get("toolCallId"))
                })
                .and_then(Value::as_str)
                .unwrap_or_default();
            let output = message_text(&event.data);
            let failed = event
                .data
                .get("isError")
                .and_then(Value::as_bool)
                .or_else(|| {
                    event
                        .data
                        .get("message")
                        .and_then(|message| message.get("isError"))
                        .and_then(Value::as_bool)
                })
                .or_else(|| {
                    event
                        .data
                        .get("meta")
                        .and_then(|meta| meta.get("isError"))
                        .and_then(Value::as_bool)
                })
                .unwrap_or(false);
            if let Some(index) = calls.get(call_id) {
                let row = &mut rows[*index];
                if !output.is_empty() {
                    if !row.text.is_empty() {
                        row.text.push('\n');
                    }
                    row.text.push_str(&output);
                }
                if failed {
                    row.text.push_str("\n[错误]");
                }
                continue;
            }
        }

        if let Some(mut row) = to_row(event) {
            if event.kind == "tool/call"
                && let Some(call_id) = event.data.get("callId").and_then(Value::as_str)
            {
                row.message_id = Some(call_id.to_string());
                calls.insert(call_id.to_string(), rows.len());
            }
            rows.push(row);
        }
    }


    if let Some(live) = &view.live {
        // 正文未开始（例如模型还在 reasoning）时给个占位，避免出现空白气泡；
        // 已有思考内容时交给 reasoning 块展示。
        let text = if live.text.is_empty() {
            if live.reasoning.is_empty() {
                "…".to_string()
            } else {
                String::new()
            }
        } else {
            live.text.clone()
        };
        let mut row =
            MessageRow::new(format!("live-{}", live.attempt_id), Role::Assistant, text)
                .streaming(true);
        if !live.reasoning.is_empty() {
            row = row.reasoning(live.reasoning.clone());
        }
        rows.push(row);
    }
    rows
}

/// 只把「会话表面」事件渲染成消息：用户输入、助手回复、工具调用/结果。
///
/// 上游把 `user/message` / `assistant/message` / `tool/result` 定为 surface 事件
/// （`packages/session/session-format-v0-to-v1/src/relationships.ts` 的 `SURFACE_TYPES`）。
/// `system/message`、`request/*`、`agent/inbox/*`、`session/*`、`step/*` 属于运行期
/// 内部事件，渲染出来就是满屏 JSON / 系统提示，必须过滤。
fn to_row(event: &SessionWireEvent) -> Option<MessageRow> {
    let (role, text, tool) = match event.kind.as_str() {
        "user/message" => {
            // 运行时注入的上下文（runtime-context / system-reminder / 时间采样）不是用户发言
            let source = event
                .data
                .get("source")
                .and_then(|value| value.get("kind"))
                .and_then(Value::as_str);
            if source != Some("user") {
                return None;
            }
            (Role::User, message_text(&event.data), None)
        }
        "assistant/message" => (Role::Assistant, message_text(&event.data), None),
        "tool/call" => {
            let name = event
                .data
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_string();
            // 卡片标题已经有工具名，正文只留参数；edit/write 直接变成 diff
            let arguments = event
                .data
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let body = diff_body(&name, arguments).unwrap_or_else(|| arguments.to_string());
            (Role::Tool, body, Some(name))
        }
        "tool/result" => (Role::Tool, message_text(&event.data), None),
        // 回合以错误结束时不会有 assistant 消息：兜底成一条系统提示，
        // 否则用户只觉得「没回答」却看不到原因。
        "turn/end" => {
            let error = event
                .data
                .get("reason")
                .and_then(|reason| reason.get("error"))?;
            let code = error
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or("error");
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("未知错误");
            let first_line = message.lines().next().unwrap_or(message);
            (
                Role::System,
                format!("{code}: {}", truncate(first_line, 300)),
                None,
            )
        }
        _ => return None,
    };

    let reasoning = if role == Role::Assistant {
        message_reasoning(&event.data)
    } else {
        None
    };
    if text.trim().is_empty() && !matches!(role, Role::Tool) && reasoning.is_none() {
        return None;
    }

    let mut row = MessageRow::new(format!("e-{}", event.seq), role, text);
    row.message_id = event
        .data
        .get("id")
        .or_else(|| event.data.get("message").and_then(|message| message.get("id")))
        .or_else(|| event.data.get("messageId"))
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(name) = tool {
        row = row.tool(name);
    }
    if let Some(reasoning) = reasoning {
        row = row.reasoning(reasoning);
    }
    Some(row)
}

/// `edit` / `write` 的参数 → unified diff 文本，交给工具卡的 diff 视图渲染。
fn diff_body(name: &str, arguments: &str) -> Option<String> {
    let value: Value = serde_json::from_str(arguments).ok()?;
    let path = value.get("file_path").and_then(Value::as_str).unwrap_or("file");
    let mut body = String::new();
    match name {
        "edit" => {
            let old = value.get("old_string").and_then(Value::as_str)?;
            let new = value.get("new_string").and_then(Value::as_str)?;
            body.push_str(&format!("--- a/{path}\n+++ b/{path}\n@@\n"));
            for line in old.lines() {
                body.push('-');
                body.push_str(line);
                body.push('\n');
            }
            for line in new.lines() {
                body.push('+');
                body.push_str(line);
                body.push('\n');
            }
        }
        "write" => {
            let content = value.get("content").and_then(Value::as_str)?;
            body.push_str(&format!("--- /dev/null\n+++ b/{path}\n@@\n"));
            for line in content.lines() {
                body.push('+');
                body.push_str(line);
                body.push('\n');
            }
        }
        _ => return None,
    }
    Some(body)
}

/// 后台任务：`job/registered` 建条目，`job/settled` 补状态与结束时间。
pub fn jobs(view: &SessionView) -> Vec<JobRow> {
    let mut rows: Vec<JobRow> = Vec::new();
    for event in &view.entries {
        let data = &event.data;
        let Some(id) = data.get("jobId").and_then(Value::as_str) else {
            continue;
        };
        match event.kind.as_str() {
            "job/registered" => {
                if rows.iter().any(|row| row.id == id) {
                    continue;
                }
                rows.push(JobRow {
                    id: id.to_string(),
                    kind: text_at(data, "kind"),
                    label: text_at(data, "label"),
                    status: "running".to_string(),
                    started_at: data.get("startedAt").and_then(Value::as_u64).unwrap_or(0),
                    finished_at: None,
                    detail: None,
                });
            }
            "job/settled" => {
                if let Some(row) = rows.iter_mut().find(|row| row.id == id) {
                    row.status = text_at(data, "status");
                    row.finished_at = data.get("finishedAt").and_then(Value::as_u64);
                    row.detail = Some(text_at(data, "detail")).filter(|text| !text.is_empty());
                }
            }
            _ => {}
        }
    }
    rows
}

/// 最近一次 `exit_plan_mode` 的计划正文（右栏计划面板）。
pub fn plan_markdown(view: &SessionView) -> Option<String> {
    view.entries.iter().rev().find_map(|event| {
        if event.kind != "tool/call" {
            return None;
        }
        let name = event.data.get("name").and_then(Value::as_str)?;
        if !name.contains("plan") {
            return None;
        }
        let arguments = event.data.get("arguments").and_then(Value::as_str)?;
        let value: Value = serde_json::from_str(arguments).ok()?;
        value
            .get("plan")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|text| !text.trim().is_empty())
    })
}

fn text_at(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// 取一条消息的可显示文本：只保留 `type == "text"` 的内容块，
/// reasoning / 图片等其它块不进正文。
fn message_text(data: &Value) -> String {
    if let Some(content) = data
        .get("content")
        .or_else(|| data.get("message").and_then(|message| message.get("content")))
    {
        let joined = join_text_parts(content);
        if !joined.is_empty() {
            return joined;
        }
    }
    data.get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// 取消息里的思考块（`{type:"reasoning", text}`）；没有则返回 `None`。
fn message_reasoning(data: &Value) -> Option<String> {
    let content = data
        .get("content")
        .or_else(|| data.get("message").and_then(|message| message.get("content")))?;
    let parts = content.as_array()?;
    let joined = parts
        .iter()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("reasoning"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<String>();
    (!joined.trim().is_empty()).then_some(joined)
}

fn join_text_parts(content: &Value) -> String {
    let Some(parts) = content.as_array() else {
        return content.as_str().unwrap_or_default().to_string();
    };
    parts
        .iter()
        .filter(|part| {
            part.get("type")
                .and_then(Value::as_str)
                .is_none_or(|kind| kind == "text")
        })
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("")
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let cut: String = text.chars().take(max_chars).collect();
        format!("{cut}…")
    }
}

fn short_id(id: &str) -> String {
    // session-<uuid> → 前 8 位
    let tail = id.rsplit('-').next().unwrap_or(id);
    tail.chars().take(8).collect()
}

fn format_time(ms: u64) -> String {
    // 只做粗粒度显示，精确格式化留到 M3
    let secs = ms / 1000;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(secs);
    // 用语言中立的短格式（本地化留给 UI 层）
    let diff = now.saturating_sub(secs);
    match diff {
        0..=59 => "now".to_string(),
        60..=3599 => format!("{}m", diff / 60),
        3600..=86399 => format!("{}h", diff / 3600),
        _ => format!("{}d", diff / 86400),
    }
}

/// `user-questions/request` waterfall → 提问卡。
///
/// 请求形状（上游 `AskUserQuestionRequest`）：`{ callId?, questions: [...] }`，
/// 每个问题 `{ id, question, detail?, options?: [{label}], multiSelect? }`。
pub fn question(event_id: String, agent_id: String, request: &Value) -> QuestionCard {
    let call_id = request
        .get("callId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let items = request
        .get("questions")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|item| {
                    let id = item.get("id").and_then(Value::as_str)?.to_string();
                    let question = item.get("question").and_then(Value::as_str)?.to_string();
                    let options = item
                        .get("options")
                        .and_then(Value::as_array)
                        .map(|opts| {
                            opts.iter()
                                .filter_map(|opt| {
                                    opt.get("label").and_then(Value::as_str).map(str::to_string)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    Some(QuestionItemView {
                        id,
                        question,
                        detail: item
                            .get("detail")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        options,
                        multi_select: item
                            .get("multiSelect")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    QuestionCard {
        event_id,
        agent_id,
        call_id,
        items,
    }
}

/// 会话视图 → 轨迹行（原始事件类型 + 数据预览 + turn/step/用量）。
pub fn trajectory(view: &SessionView) -> Vec<TrajectoryRow> {
    let mut turn = 0u32;
    let mut step = 0u32;
    view.entries
        .iter()
        .map(|event| {
            match event.kind.as_str() {
                "turn/start" => {
                    turn += 1;
                    step = 0;
                }
                "step/start" => step += 1,
                _ => {}
            }
            let data = &event.data;
            let tokens = data
                .get("usage")
                .and_then(usage_total)
                .or_else(|| data.get("tokens").and_then(Value::as_u64))
                .unwrap_or(0);
            let duration_ms = data
                .get("durationMs")
                .or_else(|| data.get("duration_ms"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            TrajectoryRow {
                seq: event.seq,
                kind: event.kind.clone(),
                detail: truncate(&data.to_string(), 120),
                turn,
                step,
                tokens,
                duration_ms,
            }
        })
        .collect()
}

fn usage_total(usage: &Value) -> Option<u64> {
    let keys = [
        "inputTokens",
        "uncachedInputTokens",
        "outputTokens",
        "cacheReadTokens",
        "cacheWriteTokens",
    ];
    let total: u64 = keys
        .iter()
        .filter_map(|key| usage.get(*key).and_then(Value::as_u64))
        .sum();
    (total > 0).then_some(total)
}

/// Host 设置 + 当前本地偏好 → 设置页快照。
pub fn settings_snapshot(
    describe: &SettingsDescribeValue,
    current_theme: Theme,
    current_locale: Locale,
) -> SettingsSnapshot {
    let theme_ns = describe.namespace(THEME_NS);
    let locale_ns = describe.namespace(LOCALE_NS);
    let subagent_ns = describe.namespace(SUBAGENT_NS);

    let theme = theme_ns
        .and_then(|item| item.str_field("preference"))
        .map(|value| match value {
            "light" => Theme::Light,
            "dark" => Theme::Dark,
            _ => Theme::System,
        })
        .unwrap_or(current_theme);
    let locale = locale_ns
        .and_then(|item| item.str_field("preference"))
        .map(|value| {
            if value == "en" {
                Locale::En
            } else {
                Locale::Zh
            }
        })
        .unwrap_or(current_locale);

    SettingsSnapshot {
        writable: describe.writable,
        theme,
        locale,
        font_size: theme_ns
            .and_then(|item| item.u64_field("fontSize"))
            .unwrap_or(14),
        code_font_size: theme_ns
            .and_then(|item| item.u64_field("codeFontSize"))
            .unwrap_or(12),
        text_font_family: theme_ns
            .and_then(|item| item.str_field("textFontFamily"))
            .unwrap_or_default()
            .to_string(),
        code_font_family: theme_ns
            .and_then(|item| item.str_field("codeFontFamily"))
            .unwrap_or_default()
            .to_string(),
        max_depth: subagent_ns.and_then(|item| item.u64_field("maxDepth")),
        max_active_subagents: subagent_ns.and_then(|item| item.u64_field("maxActiveSubagents")),
        theme_revision: theme_ns.map(|item| item.revision),
        locale_revision: locale_ns.map(|item| item.revision),
        subagent_revision: subagent_ns.map(|item| item.revision),
    }
}

/// 搜索结果 → 侧栏命中。
pub fn search_hits(value: &SessionSearchValue) -> Vec<SearchHit> {
    value
        .items
        .iter()
        .map(|item| SearchHit {
            session_id: item.session_id.as_str().to_string(),
            snippet: item.snippet.clone(),
        })
        .collect()
}

/// 工作区 → 侧栏分组头。
pub fn workspace_tabs(items: &[WorkspaceView]) -> Vec<WorkspaceTab> {
    items
        .iter()
        .map(|item| WorkspaceTab {
            id: item.workspace_id.clone(),
            title: item.title.clone(),
            path: item.path.clone(),
        })
        .collect()
}

/// 模型目录 → 设置页/输入区的分组。
pub fn model_groups(catalog: &ModelCatalog) -> Vec<ModelGroupView> {
    catalog
        .groups
        .iter()
        .map(|group| ModelGroupView {
            id: group.id.clone(),
            name: group.name.clone(),
            models: group
                .models
                .iter()
                .map(|model| ModelItemView {
                    provider: group.id.clone(),
                    model: model.id.clone(),
                    name: if model.name.is_empty() {
                        model.id.clone()
                    } else {
                        model.name.clone()
                    },
                    description: model.description.clone(),
                })
                .collect(),
        })
        .collect()
}

/// 权限预设目录。
pub fn permissions(catalog: &PermissionCatalog) -> Vec<PermissionItemView> {
    catalog
        .options
        .iter()
        .map(|option| PermissionItemView {
            value: option.value.clone(),
            name: if option.name.is_empty() {
                option.value.clone()
            } else {
                option.name.clone()
            },
            description: option.description.clone(),
        })
        .collect()
}

/// Agent preset 目录。
pub fn agent_presets(roster: &AgentPresetRoster) -> Vec<AgentPresetItemView> {
    roster
        .presets
        .iter()
        .map(|preset| AgentPresetItemView {
            id: preset.id.clone(),
            name: preset.name.clone().unwrap_or_else(|| preset.id.clone()),
            description: preset.description.clone(),
            is_default: preset.is_default,
            broken: preset.broken.clone(),
        })
        .collect()
}

/// 命令目录。
pub fn commands(items: &[CommandDescriptor]) -> Vec<CommandItemView> {
    items
        .iter()
        .map(|item| CommandItemView {
            name: item.name.clone(),
            description: item.description.clone(),
        })
        .collect()
}

/// `@` 引用候选。
pub fn file_refs(items: &[FileReferenceCandidate]) -> Vec<FileRefItemView> {
    items
        .iter()
        .map(|item| FileRefItemView {
            path: item.path.clone(),
            is_dir: item.kind == "directory",
        })
        .collect()
}

/// 计划模式 projection。
pub fn plan_state(store: &ProjectionStore, session_id: &str) -> PlanState {
    let Some(plan) = store.get(session_id, "plan") else {
        return PlanState::default();
    };
    PlanState {
        active: plan.get("active").and_then(Value::as_bool).unwrap_or(false),
        pending: plan
            .get("pending")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

/// 会话标题（来自 `title` projection；未设置时返回 None）。
pub fn title(view: &SessionView) -> Option<String> {
    view.projection("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// 待发消息（来自 `inbox` projection 的 `next-turn` 数组）。
///
/// 条目形状上游未固定导出，这里兼容 `{id, text}` / `{id, content:[{text}]}` / 纯字符串。
pub fn queue_items(view: &SessionView) -> Vec<QueueItem> {
    let Some(inbox) = view.projection("inbox") else {
        return Vec::new();
    };
    let Some(list) = inbox.get("next-turn").and_then(Value::as_array) else {
        return Vec::new();
    };

    list.iter()
        .enumerate()
        .map(|(index, entry)| {
            let id = entry
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("q-{index}"));
            let text = entry
                .get("text")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    entry.get("content").and_then(Value::as_array).map(|parts| {
                        parts
                            .iter()
                            .filter_map(|part| part.get("text").and_then(Value::as_str))
                            .collect::<Vec<_>>()
                            .join("")
                    })
                })
                .or_else(|| entry.as_str().map(str::to_string))
                .unwrap_or_default();
            QueueItem { id, text }
        })
        .collect()
}

/// 上下文占用（`contextPressure`）：返回 `(surfaceTokens, contextWindow)`。
pub fn context_usage(view: &SessionView) -> Option<(u64, u64)> {
    let pressure = view.projection("contextPressure")?;
    let surface = pressure.get("surfaceTokens")?.as_u64()?;
    let window = pressure.get("contextWindow").and_then(Value::as_u64)?;
    (window > 0).then_some((surface, window))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(kind: &str, data: Value) -> SessionWireEvent {
        serde_json::from_value(json!({
            "type": kind, "seq": 1, "time": 1, "data": data
        }))
        .unwrap()
    }

    #[test]
    fn user_message_with_text_field() {
        let row = to_row(&event(
            "user/message",
            json!({ "text": "hello", "source": { "kind": "user" } }),
        ))
        .unwrap();
        assert_eq!(row.role, Role::User);
        assert_eq!(row.text, "hello");
    }

    #[test]
    fn runtime_context_user_messages_are_hidden() {
        for source in ["runtime-context", "system-reminder"] {
            let hidden = to_row(&event(
                "user/message",
                json!({ "text": "injected", "source": { "kind": source } }),
            ));
            assert!(hidden.is_none(), "{source} 不应出现在对话里");
        }
    }

    #[test]
    fn assistant_message_with_content_parts() {
        let row = to_row(&event(
            "assistant/message",
            json!({
                "message": {
                    "content": [
                        { "type": "reasoning", "text": "先想一想" },
                        { "type": "text", "text": "he" },
                        { "type": "text", "text": "llo" }
                    ]
                }
            }),
        ))
        .unwrap();
        assert_eq!(row.role, Role::Assistant);
        assert_eq!(row.text, "hello", "reasoning 不进正文");
        assert_eq!(row.reasoning.as_deref(), Some("先想一想"));
    }

    #[test]
    fn assistant_message_with_only_reasoning_is_kept() {
        let row = to_row(&event(
            "assistant/message",
            json!({
                "message": { "content": [{ "type": "reasoning", "text": "只有思考" }] }
            }),
        ))
        .expect("只有 reasoning 的消息也应保留");
        assert!(row.text.is_empty());
        assert_eq!(row.reasoning.as_deref(), Some("只有思考"));
    }

    #[test]
    fn tool_call_keeps_raw_arguments() {
        let row = to_row(&event(
            "tool/call",
            json!({ "name": "bash", "arguments": "{\"cmd\":\"ls\"}" }),
        ))
        .unwrap();
        assert_eq!(row.role, Role::Tool);
        assert_eq!(row.tool_name.as_deref(), Some("bash"));
        assert!(row.text.contains("ls"));
    }

    #[test]
    fn control_events_are_hidden() {
        for kind in [
            "turn/start",
            "step/start",
            "request/header",
            "system/message",
            "agent/inbox/spliced",
            "session-log-deepseek/delivery-accepted",
            "session/title-llm-request",
        ] {
            assert!(
                to_row(&event(kind, json!({ "turn": 1 }))).is_none(),
                "{kind} 是内部事件，不应占消息位"
            );
        }
    }

    #[test]
    fn turn_error_becomes_system_row() {
        let row = to_row(&event(
            "turn/end",
            json!({ "turn": 1, "reason": { "error": { "code": "UNKNOWN", "message": "boom\nstack" } } }),
        ))
        .unwrap();
        assert_eq!(row.role, Role::System);
        assert!(row.text.contains("UNKNOWN"));
        assert!(row.text.contains("boom"));
        assert!(!row.text.contains('\n'), "只保留首行");
    }

    #[test]
    fn completed_turn_has_no_row() {
        assert!(
            to_row(&event("turn/end", json!({ "turn": 1, "reason": { "kind": "completed" } })))
                .is_none()
        );
    }

    #[test]
    fn edit_call_becomes_unified_diff() {
        let row = to_row(&event(
            "tool/call",
            json!({
                "callId": "c1",
                "name": "edit",
                "arguments": "{\"file_path\":\"a.rs\",\"old_string\":\"let a = 1;\",\"new_string\":\"let a = 2;\"}"
            }),
        ))
        .unwrap();
        assert!(row.text.contains("-let a = 1;"));
        assert!(row.text.contains("+let a = 2;"));
        assert!(row.text.starts_with("--- a/a.rs"));
    }

    #[test]
    fn jobs_track_registered_and_settled() {
        let mut view = SessionView::new();
        view.entries = vec![
            event(
                "job/registered",
                json!({ "jobId": "j1", "kind": "bash", "label": "cargo test", "startedAt": 1 }),
            ),
            event(
                "job/settled",
                json!({ "jobId": "j1", "status": "completed", "finishedAt": 9 }),
            ),
        ];
        let rows = jobs(&view);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "completed");
        assert_eq!(rows[0].finished_at, Some(9));
    }

    #[test]
    fn tool_call_and_result_share_one_card() {
        let mut view = SessionView::new();
        view.entries = vec![
            event(
                "tool/call",
                json!({ "callId": "c1", "name": "bash", "arguments": "{\"command\":\"ls\"}" }),
            ),
            event(
                "tool/result",
                json!({
                    "message": {
                        "toolCallId": "c1",
                        "content": [{ "type": "text", "text": "/tmp\n" }]
                    }
                }),
            ),
        ];
        let rows = rows(&view);
        assert_eq!(rows.len(), 1, "调用与结果合成一张卡");
        assert_eq!(rows[0].role, Role::Tool);
        assert_eq!(rows[0].tool_name.as_deref(), Some("bash"));
        assert!(rows[0].text.contains("ls"));
        assert!(rows[0].text.contains("/tmp"));
    }

    #[test]
    fn tool_failure_marks_card() {
        let mut view = SessionView::new();
        view.entries = vec![
            event(
                "tool/call",
                json!({ "callId": "c1", "name": "bash", "arguments": "{}" }),
            ),
            event(
                "tool/result",
                json!({ "toolCallId": "c1", "isError": true, "message": { "content": [{ "type": "text", "text": "boom" }] } }),
            ),
        ];
        let rows = rows(&view);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].text.contains("boom"));
        assert!(rows[0].text.contains("[错误]"));
    }

    #[test]
    fn catalog_rows_merge_timing_and_usage() {
        let mut store = ProjectionStore::new();
        store.seed(
            "child",
            json!({
                "subagentTiming": {
                    "settledMs": 1000,
                    "active": { "since": 2000, "through": 3000 }
                },
                "tokenUsage": {
                    "uncachedInputTokens": 10,
                    "outputTokens": 20,
                    "cacheReadTokens": 3,
                    "cacheWriteTokens": 4
                }
            }),
        );

        let rows = catalog_rows(
            "parent",
            &json!([
                { "id": "child", "createdAt": 1, "mode": "continuable", "label": "worker" },
                { "id": "ext-1", "createdAt": 2, "mode": "external" }
            ]),
            &store,
        );

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].label, "worker");
        assert_eq!(rows[0].state, SubagentState::Running);
        assert_eq!(rows[0].elapsed_ms, 2000);
        assert_eq!(rows[0].tokens, 37);
        assert_eq!(rows[1].mode, SubagentMode::External);
        assert_eq!(rows[1].label, "ext-1", "无 label 时回退 session id");
    }

    #[test]
    fn summary_marks_subagent_origin() {
        let wire: WireSummary = serde_json::from_value(json!({
            "agentAvailable": true,
            "sessionId": "child",
            "updatedAt": 1,
            "running": false,
            "blank": false,
            "parentSessionId": "parent",
            "origin": "subagent"
        }))
        .unwrap();
        let summary = to_summary(&wire);
        assert!(summary.is_subagent);
        assert_eq!(summary.parent_id.as_deref(), Some("parent"));
    }

    #[test]
    fn settings_snapshot_reads_theme_and_limits() {
        let describe: SettingsDescribeValue = serde_json::from_value(json!({
            "writable": true,
            "hasDocument": true,
            "namespaces": [
                {
                    "ns": "ui-theme", "revision": 3, "applies": "live",
                    "value": { "preference": "dark", "fontSize": 16, "codeFontSize": 13 }
                },
                {
                    "ns": "locale", "revision": 2, "applies": "live",
                    "value": { "preference": "en" }
                },
                {
                    "ns": "subagent", "revision": 1, "applies": "live",
                    "value": { "maxDepth": 2, "maxActiveSubagents": 6 }
                }
            ]
        }))
        .unwrap();
        let snapshot = settings_snapshot(&describe, Theme::System, Locale::Zh);
        assert_eq!(snapshot.theme, Theme::Dark);
        assert_eq!(snapshot.locale, Locale::En);
        assert_eq!(snapshot.font_size, 16);
        assert_eq!(snapshot.code_font_size, 13);
        assert_eq!(snapshot.max_depth, Some(2));
        assert_eq!(snapshot.max_active_subagents, Some(6));
        assert_eq!(snapshot.theme_revision, Some(3));
    }

    #[test]
    fn plan_projection_maps_to_view_state() {
        let mut store = ProjectionStore::new();
        store.seed("s1", json!({ "plan": { "active": true, "pending": true } }));
        let plan = plan_state(&store, "s1");
        assert!(plan.active);
        assert!(plan.pending);
        assert_eq!(plan_state(&store, "missing"), PlanState::default());
    }
}
