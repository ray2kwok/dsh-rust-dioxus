# PR：dsh-dioxus-ui 需求调研

| 项 | 内容 |
| --- | --- |
| 文档 | 需求调研（Product Requirements / Research） |
| 项目 | `dsh-dioxus-ui` —— 用 Dioxus 重写 deepseek-harness 的 Web GUI |
| 版本 | v0.1（调研稿） |
| 日期 | 2026-11-03 |
| 上游基线 | `deepseek-harness` @ `ce7073d512`（master）、`dioxus` @ `7dfd07eb7`（main，v0.8.0-alpha.3） |

---

## 1. 背景

### 1.1 上游是什么

`deepseek-harness` 是一个 Node/pnpm monorepo：`dsh` CLI 驱动一个本地 Host 进程，Host 通过 `dsh web` 在 `127.0.0.1:<port>`（默认 3080）提供 HTTP/WebSocket 服务，浏览器加载其 Web 前端即可与本地 Agent（模型、工具、终端、文件系统、子 Agent 等）交互。

前端不是一个普通 SPA 产物，而是一套**运行时插件系统**：

- `apps/web` 只有 4 个文件 / 约 75 行，是 Vite 壳；真正的应用在 `packages/client/web`（启动内核）+ 69 个 `packages/client/*` 插件包（其中 59 个 `ui-*`）。
- 技术栈：React 18 + 自研 Cordis DI + 自研 Slots 组合 + 自研 snapshot store（内部用 Zustand/Immer）+ CSS Modules + `--dsw-*` 设计 token；无路由库、无 Tailwind。
- Host 在渲染 `index.html` 时注入 `window.__DSH_BOOT__`（插件图）、`__DSH_CONNECTION_RECOVERY__`、`__DSH_REMOTE_ACCESS__`，浏览器侧用惰性 CJS 模块表加载各插件 bundle。
- 规模：`packages/client` 的 `src` 约 20 万行 TS（不含测试），单包最大 `ui-conversation` / `ui-chat` / `ui-primitives` 均在 1 万行以上；`apps/web/tests` 有约 470 个 Playwright e2e/快照文件。

### 1.2 为什么要用 Dioxus 重写

上游前端的一切都绑定在 JS 运行时上：Node 工具链（pnpm + Vite + tsdown）、Cordis 动态插件加载、`__DSH_BOOT__` 注入的模块图、React 运行时。这带来几个结构性问题：

1. **交付重**：跑一个 Web UI 需要 Node ≥ 24.9 + pnpm 11.28.5 + 完整 node_modules；打包成桌面/单文件二进制困难。
2. **启动链路长**：index 注入 → 模块表 → Cordis 激活 → Slots 组装，任一 entry 失败整页停在 boot page（all-or-nothing）。
3. **静态语言缺失**：约 20 万行 TS 跨 69 个包，行为只能靠读码与快照测试保证。

用 Rust + Dioxus 重写前端、并以**本地 GUI（桌面应用）**为主交付形态，可以：

- 摆脱浏览器与 Node 前端工具链：应用自带窗口，双击即用，用户不需要知道端口、地址与 token；
- 用 `cargo` 单一工具链构建，产物是单个可执行文件（组件与资源内嵌），分发与回滚都简单；
- 一次性把「插件运行时」拍平成编译期组件树，去掉模块加载器与激活审计；
- 与 Rust 生态（serde 强类型协议、syntect 高亮等）对齐；同一套组件另可编译为浏览器 WASM 形态（次要目标，见 G1b）。

**前提是行为等价**：用户看到的界面、能做的事、消息流与交互结果必须与上游一致。协议（HTTP/WS）不改，Host 不改（除静态资源挂载）。

### 1.3 本项目定位

- 目标：`dsh-dioxus-ui` 作为 deepseek-harness 的**替代 GUI**，以本地桌面应用形态交付，对接同一个 Host 进程。
- 手段：`dioxus/`、`deepseek-harness/` 两个 fork clone 作为事实来源；必要时修改上游以满足联调或构建需要。
- 判定：不看代码相似度，看**可观察行为**是否一致。

---

## 2. 目标 / 非目标

### 2.1 目标

| 编号 | 目标 |
| --- | --- |
| **G1（主目标）** | **本地 GUI**：以桌面应用形态提供 dsh 的图形界面——本机启动即用、独立窗口、不依赖浏览器与 Node 前端工具链，覆盖日常「对话 + 工具 + 审批 + 会话管理」主流程 |
| G1b（次要） | 浏览器 WASM 形态：同一套组件可再次编译为浏览器前端，用于远程访问/调试；本期不作为验收项 |
| G2 | 与 Host 协议 100% 兼容：同一 Host 同时服务上游前端与 dsh-dioxus-ui，行为一致 |
| G3 | 构建与运行不依赖 Node/pnpm 前端工具链，仅 `cargo` + `dx`；对用户而言是单个可执行程序 |
| G4 | 三栏布局、主题（light/dark/system）、i18n 骨架、快捷键等基础体验对齐上游 |
| G5 | 代码结构可增量推进：功能按模块独立落地，单个模块失败不影响其他模块联调 |

### 2.2 非目标（本期明确不做）

| 项 | 原因 |
| --- | --- |
| 重写 Host / Agent 逻辑 | Host 是唯一后端，本项目只做客户端 |
| 复刻 Cordis 插件系统与 HMR 热替换 | 编译期组件树替代；插件热更新属于开发体验，不在产品范围 |
| 终端（pty） | 需要双向 uplink 流 + xterm 级渲染，二期再评估 |
| 浏览器标签页（`ui-sidebar-browser`） | 上游 Web profile 已 `disabled`，仅 Desktop 有 |
| 桌面原生能力（`openWorkspacePath`、原生目录选择等） | 依赖 Host 侧桌面/Electron 能力；本地 GUI 形态可后续评估，本期不做 |
| 插件市场安装、动态插件运行 | 依赖 Host 侧插件管理 + 前端动态加载 |
| SSR / 服务端渲染 / 离线 PWA | 纯本地工具，收益低 |
| 像素级复刻 | 以信息结构与交互一致为准，允许控件实现不同 |

---

## 3. 上游 Web UI 现状调研

### 3.1 布局骨架

根节点只渲染一次 `root` slot，`ui-layout` 声明全部帧槽位：

| 区域 | 槽位 | 内容 |
| --- | --- | --- |
| 左栏 264–420px（默认 280，折叠 56px，<768px 抽屉） | `sidebar` | workspace/会话树、新建会话、设置入口、面板列表（plugins/schedules） |
| 中央主区（keyed） | `main` | `conversation`（会话页） / `plugins` / `schedules`；`null` 即会话页 |
| 右栏（≤70% 宽，<1024px 自动折叠） | `rightbar` | 按 session 的 dock：文件树、文档预览、终端、jobs |
| 底部整宽条 | `shell.bottom` | 出厂为空，供插件使用 |
| 全帧浮层 | `shell.overlay` | toast / badge / onboarding |
| 窗口左上 | `shell.leading` | 折叠恢复按钮、新建会话（macOS 红绿灯旁） |

没有独立状态栏与命令面板：状态信息走 header 内的 `ContextMeter`、`ConnectionIndicator`；命令分 `ui-commands`（slash command）与 `ui-shortcuts`（快捷键参考）。

### 3.2 视图清单

无 URL 路由，视图切换靠 `ctx.layout.selectPanel(id)` + store 快照。实际存在的视图：

| 视图 | 归属 | 说明 |
| --- | --- | --- |
| 会话页 | `ui-conversation` | 外壳 + composer + 队列/待办 dock；内含 Chat / Trajectory 两个 tab |
| 设置 | `ui-settings-general` 壳 + 各 `ui-settings-*` | 分区：Account / General / Models / Plugins / Agent presets |
| 插件管理 | `ui-plugin-manager` | 主面板 `plugins` |
| 定时任务 | `ui-schedule` | 主面板 `schedules` |
| 右栏 tab | `ui-sidebar-{files,documentpreview,terminal,browser}`、`ui-schedule` | 文件树 / 文档预览 / 终端 / 浏览器 / 任务 |
| 会话内浮层 | `ui-approval`、`ui-user-questions`、`ui-plan`、`ui-goal`、`ui-subagent`、`ui-jobs`、`ui-model-selection`、`ui-permission-presets`、`ui-message-feedback`、`ui-deliverables`、`ui-attachment` | 审批、提问、计划、目标、子 Agent、任务、模型选择、权限预设、反馈、变更文件卡、附件 |

### 3.3 数据流（上游）

```
Host 权威状态 → Remote（HTTP unary / WS stream）→ Client model（React-free）
             → UI 适配（useSyncExternalStore）→ Conversation/Presentation → Slots → React
```

对本次重写的关键含义：**上游的数据分层是干净的**，UI 只消费「会话快照 + 增量事件」。我们不需要复刻 Zustand/Cordis，只需要复刻这条数据通路与它的语义（重连后 snapshot 整窗替换、control 用新 baseline 原子替换、普通通知不重放）。

### 3.4 规模与工作量提示

| 维度 | 上游 | 对本项目的含义 |
| --- | --- | --- |
| 前端包 | 69（59 个 UI） | 约 25–30 个真正需要移植的功能域 |
| 源码 | 约 20 万行 TS | 按 P0 主线估：核心 6–8 万行等价 Rust |
| 渲染复杂度 | Markdown + KaTeX + Shiki + diff + 终端 ANSI + JSON 树 + 图片灯箱 | 需逐项选型（见 ARCH §6） |
| 测试 | ~470 个 Playwright 文件 | 不能照搬，需建协议回放 + 手工清单 |

---

## 4. 用户与场景

**目标用户**：本机运行 Agent 的开发者。主形态为**本地 GUI**（桌面应用，单用户、本机 loopback 通信）；浏览器形态面向远程访问与调试。

| 场景 | 描述 | 频次 |
| --- | --- | --- |
| S1 冷启动 | 双击应用 → 窗口打开 → 发现/拉起本地 Host → 连接 → 加载历史会话 → 可输入（用户不接触端口与 token） | 每次 |
| S2 对话 | 输入需求 → 流式看到助手输出与工具调用 → 结果卡展开 | 高频 |
| S3 审批 | Agent 请求执行危险工具 → 弹审批 → 允许/拒绝 → 继续 | 高频 |
| S4 会话管理 | 新建/切换/重命名/归档/置顶/搜索会话 | 中频 |
| S5 中断与队列 | 长任务中途打断；发送多条排队或 steer | 中频 |
| S6 排查 | 切到 Trajectory 看 turn/step 轨迹；打开右栏看文件与文档 | 中频 |
| S7 配置 | 改模型、agent preset、语言、字号、主题 | 低频 |
| S8 断线恢复 | 笔记本休眠/网络抖动后回到页面，状态自动恢复 | 低频但关键 |

---

## 5. 功能需求

优先级：**P0** = MVP 必须；**P1** = 完整工作流；**P2** = 增强/可选。

### 5.1 连接与基础运行时

| ID | 需求 | 优先级 | 上游来源 | 验收要点 |
| --- | --- | --- | --- | --- |
| F-01 | 凭据获取全自动：应用发现/拉起本地 Host 并取得访问凭据（token → cookie），用户不接触端口与 token | P0 | `connection/browser-auth` | 首次启动无需手工输入地址；凭据失效时给出可操作提示而非白屏 |
| F-02 | 建立 `$events` 逻辑流并以其首帧 `ready` 作为「已连接」代次 | P0 | `gateway/stream-protocol` | 断线后重连成功，UI 显示连接状态变化 |
| F-05 | Host 生命周期：复用已运行实例；未运行时拉起并在退出时回收 | P0 | `apps/cli` `dsh web` | 应用启动后即可用；关闭应用不留孤儿进程（或明确提示保留） |
| F-03 | 断线自动重连（500ms→10s 指数退避 + 抖动），重连后会话整窗刷新 | P0 | `client/connection` | 拔网 10s 恢复，消息不丢不重 |
| F-04 | 全局错误提示（RPC 错误码 → 可读文案） | P1 | 各 `error.code` | 错误码未知时展示原始 code/message |

### 5.2 会话与工作区

| ID | 需求 | 优先级 | 验收要点 |
| --- | --- | --- | --- |
| F-10 | 会话列表（按 workspace 分组），显示标题/时间/状态/未读 | P0 | 与会话真实状态一致；顺序与 Host 一致 |
| F-11 | 新建会话（可指定 cwd / workspace / preset） | P0 | 新建后进入会话页并可发送 |
| F-12 | 切换会话并加载历史（`session/follow` 快照 + 分页向上加载） | P0 | 长会话滚动到顶自动加载更早消息 |
| F-13 | 会话重命名 / 归档 / 置顶 / fork | P1 | 操作后列表即时反映 |
| F-14 | 会话搜索 | P1 | 关键词命中并跳转 |
| F-15 | workspace 管理（新建/重命名/删除/排序） | P1 | 与 Host 一致 |
| F-16 | 会话状态徽标（运行中/等待审批/空闲/错误） | P1 | 与 `api-session/status` 事件同步 |

### 5.3 对话与消息渲染

| ID | 需求 | 优先级 | 验收要点 |
| --- | --- | --- | --- |
| F-20 | 消息流：user / assistant / system / developer 四类，区分 turn/step | P0 | 消息顺序、分组与上游一致 |
| F-21 | 流式输出：`assistant-stream` 的 start/chunk/end 增量渲染 | P0 | token 连续追加，无闪烁、无重复 |
| F-22 | Markdown 渲染（GFM、表格、任务列表、脚注、引用） | P0 | 常见文档结构正确 |
| F-23 | 代码块高亮 + 复制 + 折叠长代码 | P0 | 常见语言（ts/js/py/rs/sh/json）正确 |
| F-24 | 数学公式（`$...$` / `$$...$$`） | P1 | 与上游渲染结果一致（KaTeX 兼容） |
| F-25 | 工具卡：bash / read / write / edit(diff) / search / web / ask-question + 通用兜底 | P0 | 未识别工具渲染为通用卡，不崩 |
| F-26 | diff 视图（unified / 行内高亮） | P0 | 与上游 `DiffBlock` 视觉结构一致 |
| F-27 | 文件引用（`#L24` / `:24-30`）可点击、图片附件可预览 | P1 | 点击定位/打开预览 |
| F-28 | 消息操作：复制、重试、反馈（点赞/点踩） | P1 | 反馈落库（`messageFeedback/put`） |
| F-29 | 变更文件卡（deliverables） | P2 | 列出本轮改动文件并可预览 |
| F-30 | raw HTML 永不按标记渲染；危险协议（`javascript:`）禁用 | P0（安全） | 注入用例不执行 |

### 5.4 输入与命令

| ID | 需求 | 优先级 | 验收要点 |
| --- | --- | --- | --- |
| F-40 | 多行输入、Enter 发送 / Shift+Enter 换行、IME 中文输入不误触发 | P0 | 中文输入法组字期间不发送 |
| F-41 | 发送模式：queue（排队）/ steer（打断并插入） | P0 | 运行中发送行为与上游一致 |
| F-42 | 附件：图片/文件上传（`session/uploadFileBinary`）与回显 | P1 | 上传得到 receiptId 并出现在消息里 |
| F-43 | `@` 引用文件/技能、`/` 斜杠命令补全并执行 | P1 | 补全列表来自 Host（`fileReferences/list`、`commands/list`） |
| F-44 | 输入区状态：模型选择、权限预设、上下文占用（ContextMeter） | P1 | 与 Host 状态一致 |
| F-45 | 中断：`session/cancel` 立即生效 | P0 | 点击后流式停止并标记 interrupted |
| F-46 | 队列管理（查看/删除待发消息） | P2 | — |

### 5.5 交互类浮层

| ID | 需求 | 优先级 | 验收要点 |
| --- | --- | --- | --- |
| F-50 | 审批（approval）：一次/总是/拒绝；结果回 `$events/result` | P0 | 允许则工具继续，拒绝则中止且理由回传 |
| F-51 | 用户提问（ask-question）：单选/多选/自由输入 | P0 | 回答经 `userQuestions/answer` 提交，Agent 继续 |
| F-52 | 计划（plan）卡片：查看任务清单状态 | P1 | 与 plan 事件同步 |
| F-53 | 子 Agent（subagent）状态与跳转 | P2 | — |
| F-54 | 目标（goal）与调度（schedule）卡片 | P2 | — |

### 5.6 布局、主题、设置

| ID | 需求 | 优先级 | 验收要点 |
| --- | --- | --- | --- |
| F-60 | 三栏布局：左栏折叠/拖拽调宽、右栏 dock、窄屏自适应 | P0 | 1280 / 1024 / 768 三档不破版 |
| F-61 | 主题 light/dark/system + 字号（正文/代码/终端）+ 字体族 | P0 | 切换即时生效并持久化到 Host settings |
| F-62 | 设置页：General / Models / Agent presets / About | P1 | 读写 `settings/describe|update` |
| F-63 | 语言切换（zh / en，其余 P2） | P1 | 切换后界面文案即时更新 |
| F-64 | 快捷键（新建会话、切换栏、发送、打开设置等） | P2 | 与上游默认键位一致 |
| F-65 | 首帧不闪白（boot 前落地主题变量） | P1 | 深色系统下首屏直接深色 |

### 5.7 右栏与辅助视图

| ID | 需求 | 优先级 | 验收要点 |
| --- | --- | --- | --- |
| F-70 | 文件树（懒加载、搜索、右键菜单） | P1 | 与 `workspaceFiles/list` 一致 |
| F-71 | 文档预览（文本/图片/PDF） | P2 | 大文件有上限提示 |
| F-72 | Trajectory 视图（turn/step/工具/用量） | P1 | 可定位到具体 step |
| F-73 | Jobs 列表与终止 | P2 | — |
| F-74 | 终端 | P2（本期不做） | — |

### 5.8 智能体与运维类面板

| ID | 需求 | 优先级 | 说明 |
| --- | --- | --- | --- |
| F-80 | 插件管理 / 市场 | P2 | 依赖 Host 插件管理 RPC，工作量大、优先级低 |
| F-81 | 定时任务面板 | P2 | — |
| F-82 | 账号（登录/余额） | P2 | 需要 DeepSeek 账号链路 |

---

## 6. 行为一致性要求

判定「翻译正确」的硬性口径：

1. **协议等价**：同一请求报文（字段名、`args` 键名、类型）与上游前端发出的完全一致；解析上游能解析的所有帧。
2. **状态等价**：同一事件序列下，客户端持有的会话视图（消息、turn/step、工具状态、用量）与上游一致。
3. **交互等价**：同一操作产生同一 RPC 调用与同样的可见结果；不可用时给出同类提示而非静默失败。
4. **恢复等价**：断线重连后展示的窗口与上游的重连语义一致（durable 整窗替换、control baseline 替换）。
5. **容错等价**：未知事件类型（`ignorable: true`）、未知工具、未知 `meta` 结构不导致崩溃或数据错乱。

---

## 7. 非功能需求

| 类别 | 指标 | 说明 |
| --- | --- | --- |
| 启动 | 冷启动到可交互 < 3s（本机） | 含连接/拉起 Host 与首个 `session/follow` |
| 分发体积 | 单个可执行文件 ≤ 40 MB（不含系统 WebView） | 本地工具，但需控制；浏览器形态产物 ≤ 8 MB（gzip） |
| 流式 | 高频 chunk 下保持 ≥ 30fps 滚动；单帧合并渲染 ≤ 16ms | 需要节流与增量 Markdown |
| 长会话 | 2000 条消息 / 500 个工具调用可交互 | 虚拟列表 + 折叠策略 |
| 内存 | 单会话 2000 条消息进程 RSS < 400 MB | 注意快照复制 |
| 重连 | 断网恢复 ≤ 15s 自动完成 | 指数退避 + 全场 stream 重开 |
| 兼容 | Linux WebKitGTK ≥ 2.40、macOS ≥ 13、Windows 10+（WebView2） | 以系统 WebView 为准；浏览器形态另需 Chrome/Edge ≥ 120、Safari ≥ 17 |
| 安全 | 不执行消息内 HTML/脚本；token 不进日志、不落盘明文 | Host 侧有 CSP `sandbox` 与 `nosniff` |
| 可维护 | 协议层与上游 TS 类型一一对应并可追溯 | 每个 endpoint 标注上游文件行 |
| 可测试 | 协议层可脱离 UI 单测；UI 可回放录制帧 | 见 ARCH §11 |

---

## 8. 约束与依赖

| 类别 | 约束 |
| --- | --- |
| 后端 | Host 是唯一事实来源，不改协议；Host 侧最多改动「静态资源指向」 |
| 网络位置 | 桌面形态的网络请求由 Rust 原生侧发起（不经 WebView），因此不受 Origin 同源限制、也无需内嵌代理；浏览器形态才需要与 Host 同源 |
| 运行环境 | 桌面形态依赖系统 WebView（Linux WebKitGTK / macOS WKWebView / Windows WebView2）与图形环境；无显示环境不可运行 |
| 生命周期 | 应用需负责 Host 的发现与生命周期（复用已有实例或拉起 `dsh web`），用户不应手动处理端口与 token |
| 框架 | Dioxus `0.8.0-alpha.3`（alpha），要求 Rust ≥ 1.85 且 edition 2024 |
| 工具链 | 桌面目标需系统 WebView 开发库 + `dx` CLI；浏览器形态另需 `wasm32-unknown-unknown`。当前机器默认 rustc 1.67 过旧，需切换/升级 |
| 无 Node 依赖 | 构建不得引入 Node/pnpm；但**联调**需要 Node ≥ 24.9 + pnpm 11.28.5 跑 Host（或由 GUI 拉起已安装的 `dsh`） |
| 协议风险 | 上游无 OpenAPI/JSON Schema，`args` 键名来自形参反射，需实测校验（见 §12） |

---

## 9. 里程碑

| 阶段 | 目标 | 退出标准 |
| --- | --- | --- |
| M0 打通 | 桌面窗口骨架 + 协议层 + 连接 + 会话列表 + 纯文本消息 | 双击应用即可打开本地窗口，连上 Host，读到历史会话并发出第一条消息 |
| M1 可对话 | 流式渲染 + Markdown/代码/diff + 工具卡 + 中断 | S2 场景端到端跑通 |
| M2 可批复 | 审批 / 提问 / 队列 / steer | S3、S5 跑通 |
| M3 可用 | 三栏布局 + 主题 + 会话管理 + 设置 + i18n | 日常使用不再回退上游前端 |
| M4 完整 | 右栏文件/文档、Trajectory、附件与引用、搜索 | S1–S7 全部通过 |
| M5 打磨 | 虚拟列表、长会话、重连鲁棒性、体积优化 | 达到 §7 指标 |

---

## 10. 验收标准（DoD）

- [ ] 单一 Host（`dsh web`）可同时服务上游前端与 dsh-dioxus-ui，互不影响
- [ ] S1–S7 手工清单全部通过（每项附操作步骤与预期）
- [ ] 协议回放测试覆盖 P0 全部 endpoint 与帧类型
- [ ] 断网/休眠恢复后会话内容与操作可用
- [ ] 无 raw HTML 注入执行；token 不出现在日志或磁盘明文
- [ ] `dx build --platform desktop --release` 可在干净机器复现，产物为单文件且双击可用；浏览器形态另行验证
- [ ] 文档：`ARCH.md` 与实现同步；每个模块有「上游对照文件」注释

---

## 11. 风险

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| Dioxus 0.8 alpha API 变动 | 返工 | 锁定 `=0.8.0-alpha.3`；组件层与框架 API 隔离；必要时回退 0.7.10 stable |
| 工具链不满足（1.67 默认） | 无法构建 | 升级/切换 toolchain，锁 `rust-toolchain.toml` |
| `args` 键名靠反射生成、无清单 | 协议调用 400/404 | 抓包校验 + 生成对照表 + 单测断言 |
| 上游 UI 无行为文档 | 语义偏差 | 以源码为准；每个视图建立「上游文件 ↔ 本实现」映射 |
| 渲染子系统（高亮/公式/虚拟列表）无 Rust 等价物 | 体积/性能 | 分档实现（见 ARCH §6），P0 先用最简可行方案 |
| CSS 迁移量大（`--dsw-*` + 100+ 组件样式） | 工时长 | 直接复用上游 token 与 CSS，按需裁剪 |
| 长会话性能 | 卡顿 | 虚拟列表 + 折叠 + 增量解析缓存 |

---

## 12. 待确认问题（需实测）

1. `session/list` 的 `args` 键是否真为 `{"_request":{}}`（来自单测调用，需真实 Host 抓包确认）。
2. 各 endpoint 的 `args` 键集合（`commands/execute`、`workspaceFiles/read`、`terminal/attach`、`fileUploads/upload` 等）——从 `lib/typert.host.js` dump 或抓包。
3. `permessage-deflate` 协商行为（Host 默认 `deflate`，Rust WS 库是否默认请求、协商是否影响二进制正确性）。
4. 二进制结果（multipart）在 Rust 端的解析顺序假设（按 part 名而非顺序）。
5. `terminal` 的 `id` 校验规则（客户端 mint）。
6. Web profile 下各 namespace 的实际可用方法集（缺失 provider 时的错误形态）。

---

## 13. 附：调研依据（关键路径）

> 路径相对 `deepseek-harness/`

| 主题 | 文件 |
| --- | --- |
| 启动链路 | `apps/cli/src/{bin,args,profile-boot}.ts`、`packages/bundle/web-app/{cordis.patch.yml,src/startup.ts,src/index.ts}` |
| HTTP / index 注入 | `packages/host/webserver/src/{index,injections}.ts`、`packages/host/frontend-static/src/index.ts` |
| RPC 信封 | `packages/client/connection/src/{rpc,rpc-schema,rpc-host,api-path}.ts` |
| 鉴权 | `packages/client/connection/src/{browser-auth,api-request-trust,connect-protocol}.ts` |
| WS 流协议 | `packages/api/gateway/src/{stream-protocol,index}.ts` |
| 会话契约 | `packages/api/session-controller/src/{index,types}.ts` |
| 事件类型 | `packages/core/session/src/types.ts`（`SessionEventMap`） |
| 前端架构 | `packages/client/web/src/{boot,boot-client,mount}.ts`、`packages/client/AGENTS.md` |
| 布局/槽位 | `packages/client/ui-layout/src/client/{index.ts,AppFrame.tsx}`、`packages/client/ui-slots/README.md` |
| 渲染管线 | `packages/client/ui-primitives/src/markdown/*`、`packages/client/ui-tool/src/client/tool/toolviews/*` |
| 架构文档 | `docs/api-gateway.md`、`docs/subsystems/web-client.md`、`docs/subsystems/slots.md`、`docs/web-styling.md` |

> Dioxus 侧依据见 `ARCH.md` §2。
