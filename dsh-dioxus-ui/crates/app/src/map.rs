//! 把协议类型映射成 UI 需要的视图类型。
//!
//! 刻意不做「精确建模」：`SessionWireEvent.data` 的形状由事件类型决定，
//! 而具体消息形状要等真实 prompt 事件才能确认。这里按
//! 「已知字段优先、未知结构降级」的顺序提取，保证任何事件都能渲染出东西。

use dsh_proto::session::{SessionSummary as WireSummary, SessionWireEvent};
use dsh_state::SessionView;
use dsh_ui::{MessageRow, Role, SessionStatus, SessionSummary};
use serde_json::Value;

/// 会话摘要 → 列表项。
pub fn to_summary(wire: &WireSummary) -> SessionSummary {
    // Host 不返回标题；先用 cwd 的末段，M3 接 projections 里的标题。
    let title = wire
        .cwd
        .as_deref()
        .and_then(|cwd| cwd.rsplit('/').find(|seg| !seg.is_empty()))
        .map(str::to_string)
        .unwrap_or_else(|| short_id(wire.session_id.as_str()));

    // M0 只有「运行中/空闲」两态；等待审批需要 api-session/status 事件，M2 接入
    let status = if wire.running {
        SessionStatus::Running
    } else {
        SessionStatus::Idle
    };

    SessionSummary {
        id: wire.session_id.as_str().to_string(),
        title,
        workspace: wire.cwd.clone().unwrap_or_else(|| "未分组".to_string()),
        status,
        updated: format_time(wire.updated_at),
    }
}

/// 会话视图 → 消息行；末尾追加正在流式输出的行。
pub fn rows(view: &SessionView) -> Vec<MessageRow> {
    let mut rows: Vec<MessageRow> = view.entries.iter().filter_map(to_row).collect();

    if let Some(live) = &view.live {
        rows.push(
            MessageRow::new(
                format!("live-{}", live.attempt_id),
                Role::Assistant,
                live.text.clone(),
            )
            .streaming(true),
        );
    }
    rows
}

fn to_row(event: &SessionWireEvent) -> Option<MessageRow> {
    let kind = event.kind.as_str();
    let role = role_of(kind);
    let text = text_of(event, kind);

    // 纯控制类事件不占消息位
    if text.is_empty() && !matches!(role, Role::Tool) {
        return None;
    }

    let mut row = MessageRow::new(format!("e-{}", event.seq), role, text);
    if matches!(role, Role::Tool)
        && let Some(name) = event.data.get("name").and_then(Value::as_str)
    {
        row = row.tool(name.to_string());
    }
    Some(row)
}

fn role_of(kind: &str) -> Role {
    match kind {
        k if k.starts_with("user/") => Role::User,
        k if k.starts_with("assistant/") => Role::Assistant,
        "tool/call" | "tool/result" => Role::Tool,
        _ => Role::System,
    }
}

/// 从事件里尽量取可读文本；取不到就退化成紧凑 JSON 预览。
fn text_of(event: &SessionWireEvent, kind: &str) -> String {
    let data = &event.data;

    // 1) 直接文本字段
    if let Some(text) = data.get("text").and_then(Value::as_str) {
        return text.to_string();
    }

    // 2) 内嵌 message.content 数组
    if let Some(content) = data.get("message").and_then(|m| m.get("content")) {
        let joined = join_text_parts(content);
        if !joined.is_empty() {
            return joined;
        }
    }

    // 3) 工具调用：把原始 arguments 直接展示（上游也是原始 JSON 字符串）
    if kind == "tool/call" {
        let name = data.get("name").and_then(Value::as_str).unwrap_or("tool");
        let args = data
            .get("arguments")
            .and_then(Value::as_str)
            .unwrap_or_default();
        return format!("{name} {args}");
    }

    // 4) 兜底：紧凑 JSON 预览
    let compact = data.to_string();
    if compact == "{}" || compact == "null" {
        return String::new();
    }
    truncate(&compact, 200)
}

fn join_text_parts(content: &Value) -> String {
    let Some(parts) = content.as_array() else {
        return content.as_str().unwrap_or_default().to_string();
    };
    parts
        .iter()
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
    let diff = now.saturating_sub(secs);
    match diff {
        0..=59 => "刚刚".to_string(),
        60..=3599 => format!("{} 分钟前", diff / 60),
        3600..=86399 => format!("{} 小时前", diff / 3600),
        _ => format!("{} 天前", diff / 86400),
    }
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
        let row = to_row(&event("user/message", json!({ "text": "hello" }))).unwrap();
        assert_eq!(row.role, Role::User);
        assert_eq!(row.text, "hello");
    }

    #[test]
    fn assistant_message_with_content_parts() {
        let row = to_row(&event(
            "assistant/message",
            json!({ "message": { "content": [{ "text": "he" }, { "text": "llo" }] } }),
        ))
        .unwrap();
        assert_eq!(row.role, Role::Assistant);
        assert_eq!(row.text, "hello");
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
    fn control_events_without_text_are_hidden() {
        assert!(to_row(&event("turn/start", json!({ "turn": 1 }))).is_some());
        assert!(to_row(&event("request/header", json!({}))).is_none());
    }
}
