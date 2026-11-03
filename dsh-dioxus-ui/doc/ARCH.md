# ARCH：dsh-dioxus-ui 架构设计

| 项 | 内容 |
| --- | --- |
| 文档 | 架构设计（Architecture Design） |
| 项目 | `dsh-dioxus-ui` —— Dioxus 实现的 deepseek-harness 本地 GUI |
| 版本 | **v0.2**（本地 GUI 形态；v0.1 为浏览器 WASM 形态） |
| 日期 | 2026-11-03 |
| 关联 | 需求与范围见 `PR.md` |
| 上游基线 | `deepseek-harness` @ `ce7073d512`；`dioxus` @ `7dfd07eb7`（0.8.0-alpha.3） |

---

## 1. 设计目标与原则

1. **本地 GUI 优先**：交付形态是桌面可执行程序，双击即用；浏览器形态留作同源代码的次级目标，不参与本期验收。
2. **协议零发明**：所有网络交互沿用 Host 的 HTTP RPC 与 WebSocket mux 协议，只在 Rust 侧重述类型。
3. **编译期取代运行时**：上游的 Cordis 插件图 / Slots 注册表 / 动态 bundle 加载，全部变成静态 crate 依赖与枚举分支。
4. **单向数据流**：`Host → Transport → State（归约）→ View（只读订阅）`，组件不持有连接、不发散状态。
5. **资源优先**：用系统 WebView 而不是自带渲染引擎；本机 loopback 不启用 TLS；不引入 Node/JS 构建链；能用编译期常量就不放运行时数据。
6. **可增量交付**：每个功能域一个目录、一个上游对照文件，单模块失败不影响其他模块。

> 取舍口径：功能正确性优先于体积；在功能等价的前提下，选择依赖更少、常驻内存更低的实现。

---

## 2. 关键决策（ADR 摘要）

| # | 决策 | 选择 | 理由 / 备选 |
| --- | --- | --- | --- |
| D1 | 应用形态 | **`dioxus-desktop`（wry + 系统 WebView）单窗口应用** | 系统 WebView 不打包浏览器内核，内存与分发体积最小；有完整 DOM/CSS。备选 `dioxus-native`(blitz) 是实验性渲染器、CSS 子集，不可用于本项目 |
| D2 | Dioxus 版本 | `=0.8.0-alpha.3`（锁定） | 与 `dioxus/` clone 一致，可本地 patch。备选 `0.7.10` stable |
| D3 | 网络位置 | **全部在 Rust 原生侧**（`reqwest` + `tokio-tungstenite`），**不用** webview 内 fetch | 原生请求不带 `Origin`，直接绕过 Host 的 Origin 信任栅栏，无需 cookie 同源、无需内嵌代理；且传输层可在 `cargo test` 下直接跑 |
| D4 | 路由 | `dioxus-router` + **MemoryHistory**（桌面默认） | 上游本身无 URL 路由；MemoryHistory 保留类型化路由与返回栈，语义最接近 |
| D5 | 状态载体 | `Signal` 分片 + 每会话一个 `Signal<SessionView>` | Dioxus 无 `useSyncExternalStore`；分片 signal 才能做到「更新单个会话不重渲全树」 |
| D6 | 组件组合 | 传参 + `children: Element` + 少量 `enum` 分支；不实现通用 slot 注册表 | 上游 slots 为插件解耦服务，静态树无此需求 |
| D7 | 样式 | 复用上游 `--dsw-*` token 与 CSS，`asset!` 内嵌 | 视觉对齐成本最低，零运行时开销 |
| D8 | Markdown | `pulldown-cmark` → 自建 AST → `rsx!` | 不生成 HTML 字符串，天然规避 XSS；流式按块缓存 |
| D9 | 代码高亮 | `syntect`（默认语法集） | 功能优先：覆盖全、稳定。体积约 1.5 MB，可接受。若后续需要压体积再评估 `arborium`(tree-sitter) |
| D10 | 数学公式 | `document::eval` + 内嵌 KaTeX（`include_str!`） | crates.io 上 `katex`/`ratex` 都不成熟（绑定 JS / 0.0.1）；复用上游 KaTeX 资产最省事 |
| D11 | TLS | 不启用（纯 `http://127.0.0.1`） | Host 默认明文 loopback；省掉 rustls/ring（约 1 MB+ 与常驻开销）。用户显式开 `--tls-cert` 时本期不支持 |
| D12 | Host 生命周期 | 应用负责拉起 `dsh web --no-open --port 0` 并解析 stdout 里的 URL/token | 用户不接触端口与 token；端口 0 由 OS 分配，天然避免冲突 |

---

## 3. 总体架构

```
┌────────────────────────────────────────────────────────────────┐
│ dsh-dioxus-ui（单个原生可执行文件）                               │
│                                                                │
│  dsh-app        启动 / 路由(MemoryHistory) / 窗口配置 / 错误边界   │
│     │                                                          │
│  dsh-ui         布局 · 会话 · 消息 · 工具卡 · 设置 · 主题 · i18n   │
│     │  (只读 Signal/Store，发 Intent)                           │
│  dsh-state      AppState · SessionView · 事件归约 · Intent 派发    │
│     │                                                          │
│  dsh-transport  RpcClient(reqwest) · MuxConnection(tungstenite)  │
│     │           HostSupervisor(子进程) · 重连 / generation        │
│  dsh-proto      wire 类型 · endpoint 常量 · 帧枚举 · serde        │
│                                                                │
│  ┌──────────────────────────────────────────────────┐          │
│  │ 系统 WebView（wry）：只负责渲染 rsx 与事件回传        │          │
│  └──────────────────────────────────────────────────┘          │
└───────────────────────────┬────────────────────────────────────┘
                            │  http://127.0.0.1:<port>（无 Origin）
                            │  ws://127.0.0.1:<port>/api/remote.mux
┌───────────────────────────┴────────────────────────────────────┐
│ dsh web（Node 子进程，协议不变）                                 │
│  webserver → gateway → remotes → session/workspace/settings…    │
└────────────────────────────────────────────────────────────────┘
```

上游机制与本设计的映射：

| 上游机制 | 本项目对应 |
| --- | --- |
| Cordis DI（`ctx.*` 服务） | `AppCtx` 结构体 + `use_context_provider` |
| Slots 注册表（`register/renderSlot`） | 静态组件树 + `children` + `enum MainPanel` |
| Cordis 动态插件图（`__DSH_BOOT__`） | 编译期 crate 依赖（无加载器、无激活审计） |
| snapshot store（Zustand/Immer 内核） | `Signal` 分片 + `Store`（字段级订阅） |
| `ui-renderer`（唯一 ctx→React 适配处） | `AppShell` + Router（唯一装配处） |
| locale `t` seat | `Signal<Locale>` + `t!` 宏 |
| `theme-presenter`（唯一 DOM 写入方） | `apply_theme()`（唯一写 `<html data-*>` / CSS 变量处） |
| HMR（`/plugins/events` SSE） | 不实现 |

---

## 4. 运行形态与 Host 生命周期

### 4.1 为什么不用「浏览器 + 静态托管」形态

上游 Web 形态要求前端与 Host **同源**（cookie `SameSite=Strict`，且 Connection 的信任栅栏要求 `Origin` 与 Host authority 一致）。桌面应用如果仍用 webview 发请求，就会引入「内嵌代理 / 改 Host 信任配置」这类额外组件。把网络放到 Rust 原生侧后：

- 请求不带 `Origin` 头 → 信任栅栏只校验 `Host`（loopback 通过）与 `sec-fetch-site`（原生客户端没有）；
- 不需要 cookie 与页面同源，也不需要任何代理进程；
- 少一个常驻进程、少一处故障点。

### 4.2 启动序列

```
1. 应用启动 → 读本地配置（上一次 Host 地址、窗口尺寸、主题）
2. 查找 `dsh`（PATH；或用户配置的绝对路径）
   未找到 → 显示引导页（安装说明 + 手动填写地址）
3. spawn: dsh web --no-open --port 0
   读 stdout 直到出现:  dsh web: http://127.0.0.1:<port>/?token=<launchToken>
4. GET /connect?token=<launchToken>&interactions=answer
   → 302 + Set-Cookie（v2，含 PeerId）+ Location: ./api/remote.mux
   → 记下 cookie，后续所有 /api 请求与 WS 升级都带上
5. WS ws://127.0.0.1:<port>/api/remote.mux
   open $events → 首帧 ready → 建立 generation
6. 并行加载：session/list、workspace/follow、session/control
7. 渲染主界面
8. 应用退出 → 向子进程发 SIGTERM（可配置为保留）
```

要点与兜底：

| 场景 | 处理 |
| --- | --- |
| `dsh` 不在 PATH | 引导页：给出手动地址输入 + 重试；不阻塞应用启动 |
| 端口 0 分配后 URL 解析失败 | 回退到 `--port 3080` 并重试一次（**需实测 stdout 格式**） |
| Host 已在运行 | 本期不接管他人实例（拿不到 token）；显示「检测到已有实例，请在其终端复制的地址中打开或用 `--port 0` 新起一个」 |
| 子进程崩溃 | 状态栏提示 + 「重启 Host」按钮；UI 保留草稿 |
| `/connect` 行为不符预期 | 兜底用 `GET /?token=…` 取 v1 cookie（浏览器 peer）；两者都走同一 `Credential` 抽象 |

### 4.3 进程与资源

- 常驻进程：**1 个**（应用自身；Host 由它拉起）。
- 应用侧无后台线程池（复用 dioxus-desktop 内置的 tokio 多线程 runtime）。
- Host 的 Node 进程内存不可控，属上游固有成本，界面内提供「Host 状态」面板（PID / 端口 / 内存）。

---

## 5. 技术选型

| 领域 | 选择 | 版本 | 说明 |
| --- | --- | --- | --- |
| UI | `dioxus`（features `["desktop", "router"]`） | `=0.8.0-alpha.3` | 需 Rust ≥ 1.93（`dioxus-cli` 的 MSRV，高于框架的 1.85）、edition 2024；桌面端自带 tokio 多线程 runtime |
| 序列化 | `serde` + `serde_json` | 1.x | Host 全部 JSON（无 msgpack） |
| HTTP | `reqwest`（`default-features = false`，features `["json"]`，**无 TLS**） | 0.13 | 仅 loopback；省掉 rustls/ring |
| WebSocket | `tokio-tungstenite`（**无 TLS feature**） | 0.30 | 需要能自定义请求头（Cookie）；`futures-util` 拆流 |
| UUID | `uuid`（`v4`） | 1.x | `rpcId` / `streamId` / `requestId` |
| 错误 | `thiserror` | 2.x | 协议/传输/UI 三层错误 |
| 日志 | `tracing` + `tracing-subscriber` | — | 写文件 + 控制台，支持 `DSH_LOG` |
| Markdown | `pulldown-cmark` | 0.13 | GFM：表格/任务列表/脚注/删除线；比 comrak 更省 |
| 高亮 | `syntect` | 5.3 | 功能优先；语法集全量 |
| Diff | `similar` | 3.x | hunk + 行内变更 |
| ANSI | `vte` | — | 终端输出块（P2） |
| 进程 | `std::process::Command` + 读线程 | — | 拉起 Host，解析 stdout |
| 构建 | `dx`（dioxus-cli） | 与框架同版本 | `dx build --platform desktop --release` |

不引入：`wasm-bindgen`/`gloo-net`（无 wasm 目标）、`dioxus-fullstack`（axum）、任何 Node 构建链。

**体积/内存取舍说明**：

- 不选 `dioxus-native`（blitz）：省不了多少体积，但 CSS 子集会导致大量上游样式失效，功能损失大。
- 不选 webview 内 fetch + 本地代理：多一个常驻 HTTP 服务与 WS 转发，故障面更大。
- 不启用 TLS：Host 默认明文 loopback；如需远程访问（浏览器形态）再单独加 `rustls` feature。
- `syntect` 是唯一体积较大的选择（约 1.5 MB），换取「全语种高亮开箱可用」；若最终体积超标，可换 `arborium` 分语言惰性加载。

---

## 6. 工程结构

```
dsh-dioxus-ui/
├── Cargo.toml                # [workspace] members = crates/*
├── rust-toolchain.toml       # channel = "1.85.0"（或 nightly），无 wasm target 需求
├── Dioxus.toml               # default_platform = "desktop"、asset_dir、window 配置
├── doc/{PR,ARCH}.md
└── crates/
    ├── proto/                # dsh-proto：wire 类型，无 IO
    ├── transport/            # dsh-transport：HTTP/WS/mux/重连/Host 子进程
    ├── state/                # dsh-state：AppState、SessionView、归约、Intent
    ├── render/               # dsh-render：markdown/highlight/diff（纯函数）
    ├── ui/                   # dsh-ui：组件（assets/ 在本 crate 内）
    └── app/                  # dsh-app：bin、路由、窗口、启动（assets/ 在本 crate 内）
```

> **资源必须留在各自 crate 目录内**：manganis 的 `asset!` 会拒绝 crate 之外的路径
> （`Path … is outside of allowed folders`），因此样式/字体/词典按「谁用谁带」放置，
> 而不是集中在工作区根的 `assets/`。

依赖方向严格单向：`app → ui → state → transport → proto`；`render` 只被 `ui` 依赖。

`proto` 与 `render` 无 IO、无 UI 依赖，全部逻辑可在 host target 上 `cargo test`（见 §17）。

---

## 7. 协议层 `dsh-proto`

只做「类型 + 常量 + 编解码」，不含网络。

```rust
pub const REMOTE_STREAM_MUX_PATH: &str = "/api/remote.mux";
pub const REMOTE_EVENT_STREAM_ENDPOINT: &str = "$events";
pub const REMOTE_EVENT_RESULT_ENDPOINT: &str = "$events/result";

#[derive(Serialize)]
pub struct ClientRequest<'a, P> {
    #[serde(rename = "type")] pub kind: &'static str,   // "client-request"
    pub rpc_id: String,                                  // "rpcId"
    pub method: &'a str,
    pub payload: P,                                      // { "args": { .. } }
}

#[derive(Deserialize)]
pub struct ServerResponse<T> {
    #[serde(rename = "type")] pub kind: String,          // "server-response"
    pub rpc_id: String,
    pub result: RpcResult<T>,                            // { ok, value? } | { ok:false, error }
}
```

要点（已对照上游源码确认）：

1. 一元 RPC：`POST /api/<namespace>/<method>`，`content-type: application/json`。
2. 业务失败仍是 **HTTP 200**，错误在 `result.error.{code,message,details}`；`value` 在 void 时**字段缺省**（`Option<T>` + `#[serde(default)]`）。
3. `payload` 恒为 `{ "args": { … } }`，`args` 的键名 = Host 侧**形参名**（不是方法名）。例如：
   - `session/list` → `{"_request": {}}`（**待实测确认**）
   - `session/page|follow|create|prompt|cancel|projections` → `{"request": { … }}`
   - `Agent` 形参线上是 `agentId`；`Session` 形参是 `sessionId`
   - `workspaceFiles/*` 的 scope 形参线上是 `workspaceFileScopeId`
4. WS 帧全部是 JSON **文本帧**，键名严格匹配（多一个少一个都可能被判协议错误）。
5. `open` 帧的 `payload` 与一元请求**同构**：必须自带 `{ "args": … }` 外壳。
   直接传 `{"request": …}` 会被网关拒绝：
   `gateway/internal: Remote payload must contain exactly one plain-object args field`。
   代码里统一用 `MuxConnection::open_args()` 自动补壳。

P0/P1 需要的 endpoint（`args` 键带 ⚠ 的需抓包核对）：

| endpoint | args 键 | 用途 | 优先级 |
| --- | --- | --- | --- |
| `session/list` | `_request` ⚠ | 会话列表 | P0 |
| `session/create` | `request` | 新建会话 | P0 |
| `session/prompt` | `request` | 发送消息 | P0 |
| `session/cancel` | `request` | 中断 | P0 |
| `session/follow` (stream) | `request` | 打开会话（快照 + 增量 + 流式） | P0 |
| `session/page` | `request` | 向上翻历史 | P0 |
| `session/control` (stream) | — | 全 Host projection 基线/增量 | P1 |
| `session/rename` / `fork` | `request` | 重命名 / 分支 | P1 |
| `workspace/follow` (stream) | `request` ⚠ | 工作区与会话归属 | P0 |
| `settings/describe` / `update` | `request` ⚠ | 设置读写 | P1 |
| `commands/list` / `execute` | `agentId` … ⚠ | 斜杠命令 | P1 |
| `userQuestions/answer` | `agentId, callId, answer` ⚠ | 回答提问 | P0 |
| `fileReferences/list` | `agentId, …` ⚠ | `@` 引用 | P1 |
| `workspaceFiles/list` / `read` | `workspaceFileScopeId, …` ⚠ | 文件树 / 预览 | P1 |
| `$events/result` | `clientId, eventId, outcome` | 审批/waterfall 回执 | P0 |

核心帧类型（照抄上游 `stream-protocol.ts` / `session-controller/types.ts`）：

```rust
// WS mux 客户端 → Host
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MuxClientMsg {
    Open { stream_id: String, endpoint: String, payload: serde_json::Value },
    Item { stream_id: String, #[serde(skip_serializing_if = "Option::is_none")] value: Option<serde_json::Value> },
    End { stream_id: String },
    Cancel { stream_id: String },
}

// session/follow 下行（type 判别值：snapshot / event / assistant-stream）
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SessionFollowFrame {
    Snapshot { header: SessionWireHeader, cursor: u64, records: Vec<SessionHistoryRecord>,
               has_more: bool, projections: SessionProjectionBaseline },
    Event { event: SessionWireEvent },
    AssistantStream { frame: AssistantStreamFrame },
}
```

> `snapshot` 变体字段是 camelCase（`hasMore`），而判别值是 kebab-case；实际实现要逐字段写 `#[serde(rename = "hasMore")]`，不要依赖容器级 `rename_all`。

**类型可追溯**：每个 struct 上方注释 `// 上游: packages/api/session-controller/src/types.ts:455 SessionWireEvent`。

---

## 8. 传输层 `dsh-transport`

### 8.1 组件

```
HostSupervisor  // 查找/拉起 dsh 子进程；解析 URL+token；退出回收
Credential      // /connect(token) → cookie；注入到后续请求头
RpcClient       // reqwest：call(endpoint, args) -> Result<T, RpcError>
MuxConnection   // tokio-tungstenite：open(endpoint,payload) -> StreamHandle
Reconnect       // 指数退避 500ms→10s（±抖动）；generation 跟踪
Uploader        // POST /api/session/uploadFileBinary（octet-stream）
FileUrl         // /api/file?path=&v= → 本地 http URL（交给 <img>）
```

### 8.2 一元调用

```
call("session/prompt", args_json)
  → POST http://127.0.0.1:<port>/api/session/prompt
    header: Cookie: <v2 cookie>
    body:   {"type":"client-request","rpcId":uuid,"method":"session/prompt","payload":{"args":{...}}}
  ← 200 {"type":"server-response","rpcId":...,"result":{"ok":true,"value":{...}}}
```

- 超时 30s（长任务走流，不走一元）。
- 取消：`tokio::select!` + `abort handle`；组件销毁时中止。
- 并发：`reqwest::Client` 单例（连接池复用，loopback 下开销极小）。

### 8.3 流（mux）

- `open(endpoint, payload)` 生成 `streamId = uuid`，注册 `mpsc::UnboundedSender<StreamEvent>`，返回 `StreamHandle`（`cancel()` / `send_item()` / `end()`）。
- 单条 WS 承载全部流；读任务把 `item/error/end` 按 `streamId` 分发。
- 上行 `item` 仅 `terminal/attach` 等需要（本期不用）。

### 8.4 重连与 generation

```
Connected ──ws close/error──▶ Connecting ──backoff(500ms…10s, ±抖动)──▶ Connecting
     ▲                              │
     └────── $events ready 帧 ──────┘
```

- `$events` 的 `ready` 帧是**唯一** generation 来源（携带 `clientId`、`host.home`）。
- 重连恢复顺序：
  1. 重开 `$events`（新 `clientId`，旧 `eventId` 全部作废）；
  2. 重开 `session/follow`（durable 用 `snapshot` **整窗替换**，不追加）；
  3. 重开 `session/control` / `workspace/follow`（新 `baseline` 原子替换）；
  4. 普通通知（`emit`）不重放。
- 心跳：Host 每 2s Ping，客户端自动 Pong；另设 15s watchdog（无 `item` 且无 Pong）主动断开触发重连。
- **注意**：一个 v2 cookie 只能拥有一个 WebSocket（重复连接 409）。重连前必须先关旧连接。

### 8.5 Host 子进程

```rust
pub struct HostSupervisor {
    child: Option<std::process::Child>,
    base_url: UrlSocketAddr,   // http://127.0.0.1:<port>
    launch_token: String,
}
```

- 读取 stdout 用独立线程（`BufReader::lines()`），匹配前缀 `dsh web: ` 后解析 URL 与 `token` 参数。
- 退出：`child.kill()`（Unix 发 SIGTERM 后等待 2s，超时 SIGKILL）。
- stderr 收集到环形缓冲，供「诊断信息」面板展示。

---

## 9. 状态层 `dsh-state`

### 9.1 结构

```rust
pub struct AppCtx {
    pub rpc: RpcClient,
    pub mux: MuxConnection,
    pub sessions: Signal<HashMap<SessionId, Signal<SessionView>>>, // 每会话一信号
    pub list: Signal<SessionListState>,
    pub workspaces: Signal<WorkspaceState>,
    pub conn: Signal<ConnectionState>,
    pub ui: Signal<UiState>,          // 面板、侧栏宽度、主题、locale
}

pub struct SessionView {
    pub header: SessionWireHeader,
    pub entries: Vec<Entry>,          // 按 seq 升序，已处理 surfaceOp
    pub cursor: u64,
    pub has_more: bool,
    pub live: Option<LiveAttempt>,    // 流式 attempt
    pub projections: BTreeMap<String, JsonValue>,
}
```

**为什么每会话一个 `Signal`**：Dioxus 的重渲范围由 signal 订阅粒度决定。上游同样把每个 Session 当独立 journal；这样单个会话收到 200 个流式 chunk 时，只有该会话视图重渲。

### 9.2 归约（唯一写入口）

```rust
pub fn reduce(view: &mut SessionView, frame: SessionFollowFrame) {
    match frame {
        Snapshot { header, cursor, records, has_more, .. } => {
            view.header = header; view.cursor = cursor; view.has_more = has_more;
            view.entries = records.into_iter().map(Entry::from).collect();  // 整窗替换
            view.live = None;
        }
        Event { event } => apply_event(view, event),                        // 增量
        AssistantStream { frame } => apply_stream(view, frame),             // 流式，不进 entries
    }
}
```

`apply_event` 要点：

- `surfaceOp: {op:'replace', startSeq, endSeq}` → 替换区间（compaction 依赖此语义）；`append`/缺失 → 追加。
- 未知 `type` 且 `ignorable: true` → 跳过；否则记 warn，不 panic。
- `tool/call.arguments` 是**原始 JSON 字符串**：展示与解析分离，解析失败按纯文本展示。
- `assistant/message.interrupted` → 标记该轮被中断。

### 9.3 Intent（唯一出站入口）

```rust
pub enum Intent {
    Prompt { session_id, text, mode, attachments },
    Cancel { session_id },
    Approve { client_id, event_id, outcome },
    AnswerQuestion { agent_id, call_id, answer },
    LoadMore { session_id, before_seq },
    CreateSession { cwd: Option<String> },
    Rename { .. }, Archive { .. }, Pin { .. },
    UpdateSetting { namespace, patch },
}
```

`dispatch(intent)` 是除重连/Host 生命周期外唯一发 RPC 的地方；UI 只 `dispatch`。这样所有写操作都可观测、可回放。

---

## 10. 视图层 `dsh-ui`

### 10.1 布局

```
AppShell
├── Sidebar            （264–420px；折叠 56px；<768px 抽屉）
│   ├── BrandMark / NewSession
│   ├── WorkspaceTree → SessionList（虚拟列表）
│   └── Footer：Settings / Plugins / Schedules
├── MainPanel          （enum MainPanel { Conversation, Plugins, Schedules }）
│   └── ConversationPage
│       ├── ConversationHeader（标题 / ContextMeter / ConnectionIndicator）
│       ├── MessageList（虚拟列表 + 流式尾块）
│       ├── ApprovalOverlay / QuestionOverlay
│       └── Composer（输入 / 模型 / 权限 / 附件 / @ / 斜杠命令）
└── RightBar           （dock：Files / DocumentPreview / Trajectory）
```

桌面窗口：默认 1280×840，最小 960×600；窗口尺寸与位置持久化到本地配置文件（不走 Host settings）。

### 10.2 路由（MemoryHistory）

上游视图状态是内存状态机，没有 URL。本项目用 `dioxus-router` 的类型化路由 + 桌面默认的 `MemoryHistory`，既保留「返回栈 / 深链接到具体会话」的能力，又不引入浏览器 URL 语义。

```rust
#[derive(Routable, Clone, PartialEq)]
enum Route {
    #[layout(AppShell)]
        #[route("/")]                                  Home {},
        #[route("/s/:session_id")]                     Session { session_id: String },
        #[route("/settings/:section")]                 Settings { section: String },
        #[route("/plugins")]                           Plugins {},
        #[route("/schedules")]                         Schedules {},
    #[end_layout]
    #[route("/:..segments")]                           NotFound { segments: Vec<String> },
}
```

| 上游状态 | 本设计 Route | 备注 |
| --- | --- | --- |
| `activePanelId = null` 且有选中会话 | `Session { session_id }` | 主路径 |
| `activePanelId = null` 无会话 | `Home` | 空态 / 最近会话 |
| `plugins` / `schedules` | `Plugins` / `Schedules` | P2 |
| 设置分区 | `Settings { section }` | `general`/`models`/`presets`/`about` |

### 10.3 订阅纪律（对应上游 `packages/client/AGENTS.md`）

1. 组件只读 `Signal`/`Store` 的派生值；`use_effect` 仅用于副作用（滚动、焦点、主题落盘）。
2. 派生数据用 `use_memo`；列表项以 `id` 作 key，避免整表重渲。
3. 组件不持有连接，异步结果一律写回 signal。
4. 大对象传递用 `Memo` 或 id，不 clone 整个 `Vec`。

---

## 11. 渲染子系统 `dsh-render`

| 能力 | 方案 | 关键设计 |
| --- | --- | --- |
| Markdown | `pulldown-cmark` → `MdNode` → `rsx!` | 不产 HTML 字符串；`javascript:` 等协议在解析层过滤 |
| 流式增量 | 尾部块重解析，已完成块缓存 | 对应上游 `markdown/incremental.ts`；每 ~16ms 合批 |
| 代码高亮 | `syntect`，惰性初始化全局 `SyntaxSet` | 结果按 `hash(code+lang)` 缓存；>200 行先折叠不高亮 |
| 数学 | `document::eval` + 内嵌 KaTeX | 公式文本先转义再交 KaTeX；输出只在受控容器内 `dangerous_inner_html` |
| Diff | `similar::TextDiff` → hunk + 行内 tag | 与上游 `DiffBlock` 的行号/颜色结构对齐 |
| 工具卡 | `match tool_name` → 专用组件，未命中 → `GenericToolCard` | 上游 25+ node / 10+ toolview，按 P0 分批 |
| JSON 树 | 自研递归组件 | 上限深度/节点数，超出截断 |
| ANSI | `vte` → span 样式 | P2 |
| 图片 | `<img src={file_url(path)}>` + 灯箱 | 走 `/api/file?path=&v=` |

KaTeX 说明：`katex`/`ratex` crate 均不成熟（前者是 JS 绑定，后者 0.0.1）。做法是把上游的 `katex.min.js` + `katex.min.css` + WOFF2 字体用 `asset!` 打包，首次渲染公式时用 `document::eval` 注入一次（内联脚本，无网络请求），之后每题 `eval` 只做排版调用。字体加载失败时降级为等宽文本。

---

## 12. 样式与主题

- **token 复用**：直接搬上游 `packages/client/ui-theme/src/styles/` 的 `--dsw-*` 变量与全局表（base / corner-shape / focus / scrollbar / shiki / brand-font）。纯 CSS，零运行时。
- **组件样式**：优先复用上游同名 CSS（转成全局 CSS + 前缀类名，避免 CSS Modules 工具链依赖）；无法直接复用的少数动态样式用 `style:` 内联。
- **主题落地**（唯一写入点）：

```rust
pub fn apply_theme(theme: Theme) {   // 对应上游 theme-presenter.ts
    // 1) <html data-ds-dark-theme> / color-scheme
    // 2) 字号 / 字体 CSS 变量
    // 3) 窗口标题栏配色（desktop 专有）
}
```

- 首帧防闪白：配置里存的上次主题在窗口创建时即写入（`with_initial_theme`），不等 wasm/JS 执行。

---

## 13. i18n

- 字典：从上游 `packages/client/*/src/client/locales.ts` 机械抽取为 `assets/locales/{locale}.json`，`include_str!` 编译进二进制（无运行时文件读取、无网络）。
- 运行时：`Signal<Locale>` + `t!(ns, key, args…)`；缺键回落 `en` → 键名（debug 下 warn）。
- 范围：本期 zh/en；其余语言只需补 JSON。
- 持久化：写入 Host `settings`（namespace `locale`，字段 `preference`），与上游一致。

---

## 14. 关键流程

### 14.1 冷启动

```
spawn dsh web --no-open --port 0
 → 解析 "dsh web: http://127.0.0.1:<port>/?token=…"
 → GET /connect?token=…&interactions=answer → 302 + Set-Cookie
 → WS /api/remote.mux → open $events → ready（generation 建立）
 → 并行：session/list、workspace/follow、session/control
 → 选中会话 → session/follow(address, activate:true, assistantStream:true, maxMessages:500)
 → snapshot 渲染
```

### 14.2 发送消息

```
dispatch(Prompt)
 → session/prompt {requestId: uuid, sessionId, mode, content:[{type:"text",text}], clientTimeZone}
 → {accepted:true}
 ← turn/start → user/message(source.user-rpc, rpcId=requestId) → step/start
   → assistant-stream start/chunk… → assistant/message → step/end → turn/end
```

`assistant-stream` 的 `index` 必须连续；出现缺口时用 `assistant/message.stream` 兜底重建该 attempt。

### 14.3 审批

```
Host: waterfall approval/request {eventId, agentId, request}（经 $events）
UI:   展示审批卡 → 用户选择
 → POST /api/$events/result {clientId, eventId, outcome:{kind:"result", value:"allowed-once"|"rejected"}}
Host: 继续/中止；事件流出现 approval/decided、tool/result
```

`outcome.kind` ∈ `next|result|rejected`；`clientId` 必须是当前 generation 的（重连后旧 id 失效）。

### 14.4 中断

```
dispatch(Cancel) → session/cancel {sessionId} → {accepted:true}
← assistant/message(interrupted:true) 或 turn/end(reason)
```

### 14.5 断线恢复

```
ws close → ConnectionState::Connecting（状态栏提示，输入框可继续写草稿）
  backoff → 重连 WS（先确保旧 socket 已 drop，避免 409）
  → 重开 $events → ready → 新 generation
  → 所有 session/follow 重发 open → snapshot 整窗替换
  → session/control / workspace/follow 收 baseline → 替换
```

---

## 15. 错误处理与降级

| 场景 | 表现 |
| --- | --- |
| RPC 业务错误（HTTP 200 + `ok:false`） | 按 `code` 映射文案；未知 code 原样展示 `code: message` |
| 401 | 凭据失效：提示「重新连接 Host」（重跑 `/connect`），不要求重启应用 |
| 403 | 信任栅栏拒绝：诊断面板展示请求头（Host/Origin），提示用 `--trusted-host` 或改为 loopback |
| 单个流 `error` 帧 | 该流关闭；视图标记「该会话已断开」，可手动重试；不影响其他流 |
| 未知事件类型 | `ignorable:true` 跳过；否则忽略并计数（诊断面板可见） |
| 未知工具名 | `GenericToolCard`（原始 JSON 折叠） |
| Markdown/高亮异常 | 退化为纯文本 / 无高亮 |
| 大文本（>1 MB） | 截断 + 「展开全文」按需 |
| panic | 全局 `ErrorBoundary` → 错误页 + 「复制诊断信息」；不整窗空白 |

---

## 16. 性能与资源

| 关注点 | 手段 |
| --- | --- |
| 流式渲染 | chunk 进 ring buffer，每帧 flush 一次（16ms tick），不逐 chunk 重渲 |
| 长列表 | 自研虚拟列表（可见区 + overscan），消息高度测量后缓存 |
| Markdown | 已完成块缓存为 `Element`，只重解析尾部块 |
| 高亮 | 视口内才高亮；按内容 hash 缓存；超长先折叠 |
| 状态更新 | 每会话独立 signal；`Store` 字段级订阅；避免整 `Vec` clone |
| 常驻内存 | 会话窗口上限（最近 2000 条 + 按需回补）；`Element` 缓存设上限 |
| 二进制体积 | `opt-level="z"`、`lto="fat"`、`panic="abort"`、`strip`；关闭 `devtools` 等可选 feature（**需验证 facade feature 透传**） |
| 启动 | 首屏只挂载 sidebar + 当前会话；右栏与设置懒加载 |
| 子进程 | 单一 `reqwest::Client`（连接池）；不额外起线程池（复用 dioxus-desktop 的 tokio runtime） |

---

## 17. 测试策略

| 层 | 方式 | 覆盖 |
| --- | --- | --- |
| `dsh-proto` | `cargo test`，serde 往返 | 信封/帧/错误；`args` 键名断言（防回归） |
| `dsh-state` | `cargo test`，**帧回放** | 喂录制 `snapshot + event*`，断言最终 `SessionView` |
| `dsh-render` | `cargo test` | Markdown → 节点树快照；diff/ANSI 纯函数 |
| `dsh-transport` | `cargo test` + 本地 mock（`axum`/`hyper` dev-dependency 或手写 TCP） | URL/帧编解码、重连退避、cookie 注入 |
| 联调 | `dsh web` 真机（Node 可用时，标记 `#[ignore]`） | 连接、unary、stream、审批全链路 |
| UI | 手工清单 + 可选 `dioxus-desktop` headless 测试 | S1–S7 |

**帧录制**：在 Node 侧起 Host，用 Playwright 监听 `websocket` 帧与 `request/response`，按场景存 `fixtures/<scenario>.jsonl`。这把「上游行为」固化成可回归的数据，是本项目最重要的测试资产（录制脚本放在 `deepseek-harness/` 侧临时使用，不进入产物）。

---

## 18. 构建与发布

```bash
# 一次性环境（推荐：先用脚本体检，再按提示安装）
./scripts/setup-env.sh            # 只检查，输出阻塞项与修复命令；可作 CI 门禁
./scripts/setup-env.sh --install  # 逐条确认后安装（--yes 免确认，--dry-run 只看命令）

# 等价的手工步骤
rustup toolchain install stable           # 需 >= 1.93（dioxus-cli 的 MSRV）/ edition 2024
cargo install dioxus-cli --version 0.8.0-alpha.3    # 版本需与 dioxus 对齐
# Linux 另需系统 WebView 开发库（webkit2gtk-4.1 / libsoup3）

# 构建
dx build --platform desktop --release
# 产物：target/dx/dsh-dioxus-ui/<profile>/desktop/…（目录结构以 dx 实际输出为准）
```

### 18.1 本地联调 Host（源码形态）

`deepseek-harness` 直接 `pnpm install` 后并不能跑 `dsh web`，还有两道坎：

1. **Typert 描述符**：`lib/typert.host.js` 是构建产物，缺失时启动报
   `typert contributor(s) failed to register`。用 `pnpm exec tsx scripts/generate-typert.ts`
   生成（36 个包 / 约 4 秒），比整仓 `build` 快得多。
2. **前端 client bundle**：`dsh web` 的 `modules` 插件要求 63 个 `lib/client.js`。
   Rust 前端不加载它们，用 patch 关闭该行即可跳过整条前端构建链：

   ```yaml
   # patch.yml
   - id: modules
     disabled: true
   ```

   `--patch` 必须紧跟子命令（`dsh web --patch … --no-open --port 0`），
   放在 `--port` 之后会被判为 unknown option。

`scripts/dev-host.sh` 把上述步骤封装成一条命令（`DSH_REPO` 可覆盖仓库路径）。

- 资源（CSS/字体/词典）经 `asset!` 内嵌或随包分发，运行时无外部文件依赖。
- 运行前提：目标机已安装 `dsh`（Node ≥ 24.9）；未安装时走引导页。
- 回滚：应用与 Host 解耦，换回上游前端只需让用户用浏览器打开 Host 地址，无需改 Host。
- CI：`cargo fmt --check`、`cargo clippy -D warnings`、`cargo test`（proto/state/render/transport）、`dx build`。

---

## 19. 实施计划

> **进度（2026-11-03）**：
> - M0-1 ~ M0-5 完成并通过真实 Host 验证（握手 → `session/list` → `session/create`
>   → `session/follow` → `SessionView` → 渲染）；
> - M1 完成：Markdown 解析与渲染（`dsh-render` + `MarkdownView`）、diff 行解析、
>   代码高亮（零依赖扫描器，见 `dsh-render::highlight`）、通用工具卡、流式合批节流；
> - 断线自动重连已实现并实测（杀掉 Host → 500ms 退避 → 新 Host → generation+1）。
> 下一步 M2：审批 / 提问 / 队列 / steer 的交互闭环。

| 阶段 | 交付 | 关键文件 |
| --- | --- | --- |
| M0-1 | workspace 骨架、`rust-toolchain.toml`、`Dioxus.toml`、空窗口 | `Cargo.toml`、`crates/app` |
| M0-2 | `dsh-proto`：信封 + `$events` + `session/follow` 帧 + 错误 | `crates/proto` |
| M0-3 | `dsh-transport`：HostSupervisor + Credential + RpcClient + MuxConnection + 重连 | `crates/transport` |
| M0-4 | `dsh-state`：AppState/SessionView/归约 + `session/list|create|prompt` | `crates/state` |
| M0-5 | 最小 UI：会话列表 + 纯文本消息 + 输入框 | `crates/ui` |
| M1 | `dsh-render`（Markdown/代码/diff）+ 工具卡 + 流式 + 中断 | `crates/render`、`crates/ui` |
| M2 | 审批 / 提问 / 队列 / steer / overlay 层 | `crates/ui/overlays` |
| M3 | 三栏布局 + 主题 + 会话管理 + 设置 + i18n | `crates/ui/{layout,settings}` |
| M4 | 右栏文件/预览、Trajectory、附件与引用、搜索 | `crates/ui/rightbar` |
| M5 | 虚拟列表、体积与内存、重连鲁棒性、e2e 固化 | 全局 |

---

## 20. 风险与对策

| 风险 | 触发条件 | 对策 |
| --- | --- | --- |
| Linux WebView 依赖（webkit2gtk/soup3）缺失或版本过低 | 新机器运行 | 启动自检 + 明确提示；CI 用固定基础镜像 |
| 系统 WebView 差异（WKWebView / WebView2 / WebKitGTK） | 跨平台渲染不一致 | 只用稳定 CSS 子集；关键交互不依赖 `backdrop-filter` 等特性；三平台各跑一遍清单 |
| Dioxus 0.8 alpha 破坏性变更 | 升级框架 | 锁死版本；框架 API 收口在 `dsh-ui/src/platform.rs` 薄封装 |
| `--port 0` 的 stdout 格式不符预期 | 启动解析失败 | 先实测；兜底固定端口 + 冲突探测 |
| `/connect` 语义与预期不符 | 取不到 cookie | 兜底走 `GET /?token=` 取 v1 cookie；`Credential` 抽象两实现 |
| v2 cookie 单 WS 限制 | 重连 409 | 重连前确保旧 socket drop；失败则重取凭据 |
| 无 `useSyncExternalStore` 导致重渲过多 | 长会话卡顿 | 每会话 signal + `Store` lens + 虚拟列表 |
| `syntect` 体积 | 产物超预算 | 换 `arborium` 分语言惰性加载；或先支持少量语言 |
| KaTeX 依赖 JS 注入 | 公式渲染 | 内联脚本 + 失败降级为等宽文本 |
| 上游 CSS 直接复用的类名冲突 | 样式错乱 | 统一前缀 + 单文件顺序加载；不兼容处逐文件手改 |
| `args` 键名与假设不符 | 调用 4xx | M0 阶段抓包校验全部 P0 endpoint，写进单测 |
| 长文本/大 diff 渲染卡顿 | 工具输出巨大 | 阈值截断 + 按需展开；解析放最终态 |

---

## 21. 开放问题

**已实测关闭**（2026-11-03，`crates/transport/examples/probe.rs` 对真实 Host）：

- `dsh web --no-open --port 0` 的输出格式与 token 解析 ✅
- `/connect?interactions=answer` 的 cookie 可用于一元 `/api/*` ✅
- `session/list` 的 `args` 必须是 `{"_request":{}}` ✅
- `$events` 首帧 `ready`（含 `clientId`/`host.home`）可解析 ✅

**仍开放**：

1. 其余 endpoint 的 `args` 键集合（`commands/execute`、`workspaceFiles/read`、`terminal/attach`、`fileUploads/upload`），按同一方法逐个探测。
2. `permessage-deflate` 在长帧上的表现（首帧已验证正常）。
3. `assistant-stream` 是否可能乱序/缺口（上游有 `index`/`revision`；重连时能否只用 `assistant/message.stream` 重建）。
5. 上游 `--dsw-*` token 与组件 CSS 是否存在依赖 React 结构的样式（需逐文件迁移验证）。
6. Linux 上 `dx build --platform desktop` 的打包产物在无 Node 环境的机器上是否只需系统 WebView（需在干净镜像验证）。
