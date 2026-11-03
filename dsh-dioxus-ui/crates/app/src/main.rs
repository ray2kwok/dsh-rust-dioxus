//! dsh-dioxus-ui 桌面入口。
//!
//! 当前阶段（M0）：UI 骨架 + 本地 mock 数据，用于验证布局与交互回路。
//! 接入顺序见 `doc/ARCH.md` §19：HostSupervisor → transport → state → 真实数据。
//!
//! mock 数据集中在 `mock` 模块，替换为真实数据时只改这里，组件层不动。

mod mock;

use dioxus::prelude::*;
use dsh_ui::{AppShell, ConnectionState, MessageRow};

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let sessions = use_signal(mock::sessions);
    let active = use_signal(|| Some("s-1".to_string()));
    let mut messages = use_signal(mock::messages);
    let mut next_id = use_signal(|| 100u32);

    let on_select = move |id: String| {
        let mut active = active;
        active.set(Some(id));
    };

    let on_send = move |text: String| {
        let id = next_id();
        next_id.set(id + 1);
        messages.write().push(MessageRow::new(
            format!("m-{id}"),
            dsh_ui::Role::User,
            text.clone(),
        ));
        // M0 用回声占位：真实实现会 dispatch(Prompt) 并由 session/follow 推回事件
        messages.write().push(
            MessageRow::new(
                format!("m-{}", id + 1),
                dsh_ui::Role::Assistant,
                format!("（M0 占位回声，未连接 Host）收到：{text}"),
            )
            .streaming(false),
        );
    };

    rsx! {
        document::Stylesheet { href: MAIN_CSS }
        AppShell {
            sessions: sessions(),
            active_id: active(),
            messages: messages(),
            connection: ConnectionState::Disconnected,
            generation: 0,
            host_label: "未启动 Host".to_string(),
            on_select,
            on_send,
        }
    }
}
