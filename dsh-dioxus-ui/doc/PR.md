# PR：dsh-dioxus-ui 需求调研

| 项 | 内容 |
| --- | --- |
| 文档 | 需求调研（Product Requirements / Research） |
| 项目 | `dsh-dioxus-ui` —— 用 Dioxus 重写 deepseek-harness 的 Web GUI |
| 版本 | v0.2（P2 范围调整：多 Agent 协作 / 集群） |
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
| 浏览器标签页（`ui-sidebar-browser`） | 上游 Web profile 已 `disabled`，仅 Desktop 有 |
| 桌面原生能力（`openWorkspacePath`、原生目录选择等） | 依赖 Host 侧桌面/Electron 能力；本地 GUI 形态可后续评估，本期不做 |
| 插件市场安装、动态插件运行 | 依赖 Host 侧插件管理 + 前端动态加载；已移入 P3 待办池（§5.9） |
| SSR / 服务端渲染 / 离线 PWA | 纯本地工具，收益低 |
| 像素级复刻 | 以信息结构与交互一致为准，允许控件实现不同 |

---

## 3. 上游 Web UI 现状调研

### 3.1 布局骨架

根节点只渲染一次 `root` slot，`ui-layout` 声明全部帧槽位：

| 区域 | 槽位 | 内容 |
| --- | --- | --- |
| 最左活动栏 48px | `activitybar` | 资源管理器 / 设置；点击资源管理器图标可折叠左侧栏 |
| 左侧资源管理器 180–420px（默认 260） | `explorer` | 文件树 + 新建工作区（会话管理不在此处） |
| 中央编辑区 | `editor` | tab：对话 / 文件预览 / 子会话副本；对话为默认 tab |
| 中央底部面板 140–560px（默认 210） | `bottom-panel` | tab：输入（Composer）/ 终端 / 后台任务 / 计划，可折叠、可拖高 |
| 右侧会话面板 240–460px（默认 300） | `sessionpane` | 会话搜索、新建会话、工作区与会话列表、置顶/fork；可折叠、可拖宽 |
| 全宽状态栏 30px | `statusbar` | 连接状态、Host、generation、会话/终端开关、重连、设置 |

结构对齐 CodeBuddy/VS Code：活动栏 + 资源管理器 + 编辑区 + 底部集成面板 + 右侧面板 + 状态栏。dsh 没有编辑器，编辑区承载会话与只读预览，输入区在底部面板。

### 3.2 视图清单

无 URL 路由，视图切换靠 `ctx.layout.selectPanel(id)` + store 快照。实际存在的视图：

| 视图 | 归属 | 说明 |
| --- | --- | --- |
| 会话页 | `ui-conversation` | 外壳 + composer + 队列/待办 dock；内含 Chat / Trajectory 两个 tab |
| 子 Agent 目录 | `ui-subagent` | 会话 header 的后代计数触发 + 层级目录；打开子会话、续聊与独立停止（P2 主线） |
| 设置 | `ui-settings-general` 壳 + 各 `ui-settings-*` | 分区：Account / General / Models / Plugins / Agent presets |
| 插件管理 | `ui-plugin-manager` | 主面板 `plugins`（P3） |
| 定时任务 | `ui-schedule` | 主面板 `schedules`（P3） |
| 底部面板 | `bottom-panel` | 终端 / 后台任务 / 计划（替代上游右栏 tab，对齐 VS Code 集成面板） |
| 会话内浮层 | `ui-approval`、`ui-user-questions`、`ui-plan`、`ui-goal`、`ui-jobs`、`ui-model-selection`、`ui-permission-presets`、`ui-message-feedback`、`ui-deliverables`、`ui-attachment` | 审批、提问、计划、目标、任务、模型选择、权限预设、反馈、变更文件卡、附件 |

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
| S9 多 Agent 协作 | 父会话下派出多个子 Agent：从 header 目录查看集群、进入某个子会话、续聊或单独停止，再返回父会话 | 中频（P2 主线） |

---

## 5. 功能需求

优先级：**P0** = MVP 必须；**P1** = 完整工作流；**P2** = 本期主线「多 Agent 协作 / 集群」，只保留与该主线相关的需求；**P3** = 后续排期（本期不做，见 §5.9）。

> 2026-11-03 范围调整：P2 收敛为「多 Agent 协作 / 集群」一条主线。原 P2 中与多 Agent 无关且尚未实现的条目全部降级为 P3；已实现项（如 F-46 队列管理）保留 P2 标记并注明状态。P2 的拆解见 §5.5，P3 清单见 §5.9。

### 5.1 连接与基础运行时

| ID | 需求 | 优先级 | 上游来源 | 验收要点 |
| --- | --- | --- | --- | --- |
| F-01 | 凭据获取全自动：应用发现/拉起本地 Host 并取得访问凭据（token → cookie），用户不接触端口与 token | P0 | `connection/browser-auth` | 首次启动无需手工输入地址；凭据失效时给出可操作提示而非白屏 |
| F-02 | 建立 `$events` 逻辑流并以其首帧 `ready` 作为「已连接」代次 | P0 | `gateway/stream-protocol` | 断线后重连成功，UI 显示连接状态变化 |
| F-05 | Host 生命周期：复用已运行实例；未运行时拉起并在退出时回收 | P0 | `apps/cli` `dsh web` | 应用启动后即可用；关闭应用不留孤儿进程（或明确提示保留） |
| F-03 | 断线自动重连（500ms→10s 指数退避 + 抖动），重连后会话整窗刷新 | P0 | `client/connection` | 拔网 10s 恢复，消息不丢不重 |
| F-04 | 全局错误提示（RPC 错误码 → 可读文案） | P1（已实现） | 各 `error.code` | 错误码未知时展示原始 code/message |

### 5.2 会话与工作区

| ID | 需求 | 优先级 | 验收要点 |
| --- | --- | --- | --- |
| F-10 | 会话列表（按 workspace 分组），显示标题/时间/状态/未读 | P0 | 与会话真实状态一致；顺序与 Host 一致 |
| F-11 | 新建会话（可指定 cwd / workspace / preset） | P0 | 新建后进入会话页并可发送 |
| F-12 | 切换会话并加载历史（`session/follow` 快照 + 分页向上加载） | P0 | 长会话滚动到顶自动加载更早消息 |
| F-13 | 会话重命名 / 归档 / 置顶 / fork | P1（已实现） | 操作后列表即时反映 |
| F-14 | 会话搜索 | P1（已实现） | 关键词命中并跳转 |
| F-15 | workspace 管理（新建/重命名/删除/排序） | P1（已实现，按钮排序） | 与 Host 一致 |
| F-16 | 会话状态徽标（运行中/等待审批/空闲/错误） | P1（部分） | 与 `api-session/status` 事件同步 |

### 5.3 对话与消息渲染

| ID | 需求 | 优先级 | 验收要点 |
| --- | --- | --- | --- |
| F-20 | 消息流：user / assistant / system / developer 四类，区分 turn/step | P0 | 消息顺序、分组与上游一致 |
| F-21 | 流式输出：`assistant-stream` 的 start/chunk/end 增量渲染 | P0 | token 连续追加，无闪烁、无重复 |
| F-22 | Markdown 渲染（GFM、表格、任务列表、脚注、引用） | P0 | 常见文档结构正确 |
| F-23 | 代码块高亮 + 复制 + 折叠长代码 | P0 | 常见语言（ts/js/py/rs/sh/json）正确 |
| F-24 | 数学公式（`$...$` / `$$...$$`） | P1（轻量实现） | 与上游渲染结果一致（KaTeX 兼容） |
| F-25 | 工具卡：bash / read / write / edit(diff) / search / web / ask-question + 通用兜底 | P0 | 未识别工具渲染为通用卡，不崩 |
| F-26 | diff 视图（unified / 行内高亮） | P0 | 与上游 `DiffBlock` 视觉结构一致 |
| F-27 | 文件引用（`#L24` / `:24-30`）可点击、图片附件可预览 | P1（部分） | 点击定位/打开预览 |
| F-28 | 消息操作：复制、重试、反馈（点赞/点踩） | P1（已实现） | 反馈落库（`messageFeedback/put`） |
| F-29 | 变更文件卡（deliverables） | P3 | 非多 Agent 主线，见 §5.9 |
| F-30 | raw HTML 永不按标记渲染；危险协议（`javascript:`）禁用 | P0（安全） | 注入用例不执行 |

### 5.4 输入与命令

| ID | 需求 | 优先级 | 验收要点 |
| --- | --- | --- | --- |
| F-40 | 多行输入、Enter 发送 / Shift+Enter 换行、IME 中文输入不误触发 | P0 | 中文输入法组字期间不发送 |
| F-41 | 发送模式：queue（排队）/ steer（打断并插入） | P0 | 运行中发送行为与上游一致 |
| F-42 | 附件：图片/文件上传（`session/uploadFileBinary`）与回显 | P1（已实现） | 上传得到 receiptId 并出现在消息里 |
| F-43 | `@` 引用文件/技能、`/` 斜杠命令补全并执行 | P1（已实现） | 补全列表来自 Host（`fileReferences/list`、`commands/list`） |
| F-44 | 输入区状态：模型选择、权限预设、上下文占用（ContextMeter） | P1（已实现） | 与 Host 状态一致 |
| F-45 | 中断：`session/cancel` 立即生效 | P0 | 点击后流式停止并标记 interrupted |
| F-46 | 队列管理（查看/删除待发消息） | P2（已实现） | `session/updateQueue` + `inbox` projection；`remove` / `steer` 已接通 |

### 5.5 交互类浮层与子 Agent

| ID | 需求 | 优先级 | 验收要点 |
| --- | --- | --- | --- |
| F-50 | 审批（approval）：一次/总是/拒绝；结果回 `$events/result` | P0 | 允许则工具继续，拒绝则中止且理由回传 |
| F-51 | 用户提问（ask-question）：单选/多选/自由输入 | P0 | 回答经 `userQuestions/answer` 提交，Agent 继续 |
| F-52 | 计划（plan）卡片：查看任务清单状态 | P1（已实现） | 与 plan 事件同步 |
| F-53 | 子 Agent 目录（集群视图）：父会话 header 显示直接子级数量，展开为可递归的层级目录；每行展示模式（one-shot / continuable / external / unknown）、活动状态、标题、累计用量与本轮耗时 | P2 | 父会话 `subagentCatalog` 非空或读取失败时出现入口；成功加载的空目录不显示；子级按需读取其 projections |
| F-53b | 打开子会话：点击目录行，以精确地址 `{kind:"subagent", parentSessionId, childSessionId, mode}` 打开子会话；行尾箭头在右栏侧开 | P2 | 子会话的消息、流式、审批与普通会话一致；面包屑可返回父会话 |
| F-53c | 子会话续聊：continuable 子会话在父会话存活时可直接输入并发送（`subagents/prompt`，delivery 支持 queue / steer），并可独立停止当前 turn（`subagents/interruptByParent`） | P2 | 续聊进入子会话 FIFO；停止只影响子会话，不动父会话 |
| F-53d | 只读子会话：one-shot / unknown 模式，或父会话不可用且子会话已停止时，输入区降级为只读说明，不提供发送 | P2 | 只读态不发出 prompt；父离线但仍在运行的 continuable 子会话保留停止按钮 |
| F-53e | 子 Agent 实时状态：运行/完成/空闲状态点、本轮耗时、累计 token 随 projection 帧更新；嵌套层级展开后同样实时 | P2 | 不使用轮询；数据来自 `session/control` 的 baseline + `projection` 增量 |
| F-53f | 会话列表隔离：普通侧栏不出现 `origin == "subagent"` 的子会话 | P2 | `session/list` 含子会话时按 `origin` 过滤；子会话只能从父会话目录进入 |
| F-54 | 目标（goal）与调度（schedule）卡片 | P3 | 非多 Agent 主线，见 §5.9 |

**P2 可选（不阻塞 M4 退出）**：子 Agent 委派上限（`subagent` namespace 的 `maxDepth` / `maxActiveSubagents`）与子 Agent 模型选择（`subagent-model-selection`）。上游把它放在 Plugins 页；本项目若做，走 `settings/describe|mutate` 直连 namespace，不依赖插件管理页（F-80 仍在 P3）。

> Agent / Subagent 的关系、创建与管理流程、dsh 端包组合与构建步骤，见
> [`AGENT_SUBAGENT.md`](AGENT_SUBAGENT.md)。

> **集群范围的边界**：上游实验性 `agent-team` 只有 Host 侧 roster / task board 与工具标题文案，基线 Web 客户端没有团队面板。本期「集群视图」以子 Agent 目录（F-53）为准；若后续 Host 暴露 `agentTeam` projection，再在同一入口扩展。

### 5.6 布局、主题、设置

| ID | 需求 | 优先级 | 验收要点 |
| --- | --- | --- | --- |
| F-60 | CodeBuddy 式工作台：活动栏 + 文件资源管理器 + 编辑区（对话 tab）+ 底部输入/终端 + 右侧会话面板 + 状态栏 | P0（已实现） | 三处拖拽调宽/调高、底部与会话面板可折叠；1280 / 1024 不破版 |
| F-61 | 主题 light/dark/system + 字号（正文/代码/终端）+ 字体族 | P0（已实现正文/代码；终端随 P3） | 切换即时生效并持久化到 Host settings |
| F-62 | 设置页：General / Models / Agent presets / About | P1（已实现） | 读写 `settings/describe\|update` |
| F-63 | 语言切换（zh / en） | P1（已实现） | 切换后界面文案即时更新 |
| F-63b | 其余语言（ja / es / fr / de / ru / pt） | P3 | 见 §5.9 |
| F-64 | 快捷键（新建会话、切换栏、发送、打开设置等） | P3 | 非多 Agent 主线，见 §5.9 |
| F-65 | 首帧不闪白（boot 前落地主题变量） | P1（已实现） | 深色系统下首屏直接深色 |

### 5.7 编辑区与辅助视图

| ID | 需求 | 优先级 | 验收要点 |
| --- | --- | --- | --- |
| F-70 | 文件树（懒加载、搜索、右键菜单） | P1（已实现） | 左侧资源管理器；与 `workspaceFiles/list` 一致 |
| F-71 | 文档预览：文本文件 | P2（已实现） | 中间编辑区可读文本；带语法高亮与复制 |
| F-71b | 文档预览：图片 / PDF / 二进制 | P3 | 需新增解码与渲染链路，见 §5.9 |
| F-72 | Trajectory 视图（turn/step/工具/用量） | P1（已实现） | 按 turn 分组、显示 step/用量/耗时；点击回跳消息 |
| F-73 | Jobs 列表与终止 | P3 | 见 §5.9 |
| F-74 | 终端 | P2（MVP 已实现） | 底部「终端」tab：`terminal/list\|create\|attach\|close`；ANSI SGR 渲染；输入按 `sequence` 走 uplink；切会话断流但保留 Host 进程 |

### 5.8 智能体与运维类面板

| ID | 需求 | 优先级 | 说明 |
| --- | --- | --- | --- |
| F-80 | 插件管理 / 市场 | P3 | 依赖 Host 插件管理 RPC，工作量大；见 §5.9 |
| F-81 | 定时任务面板 | P3 | 见 §5.9 |
| F-82 | 账号（登录/余额） | P3 | 需要 DeepSeek 账号链路；见 §5.9 |

### 5.9 P3 待办池（本期不排期）

以下条目由原 P2 降级或本就属于后续增强，**不纳入本期里程碑**；需要时单独排期。

| ID | 需求 | 降级原因 / 前置依赖 |
| --- | --- | --- |
| F-29 | 变更文件卡（deliverables） | 依赖 deliverables 事件与预览链路；可与子会话预览共用部分能力 |
| F-54 | 目标 / 调度卡片 | 依赖 `ui-goal`、`ui-schedule` 两个独立功能域 |
| F-63b | 其余语言 | 需从上游 `locales.ts` 全量抽取字典，不影响主流程 |
| F-64 | 快捷键 | 独立基建，需与上游 `shortcuts` 默认键位逐条对齐 |
| F-71b | 图片 / PDF 预览 | 需要图片解码、PDF 渲染与二进制读取链路 |
| F-73 | Jobs 列表与终止 | 依赖 Host jobs RPC |
| F-80 | 插件管理 / 市场 | 依赖 Host 插件管理 RPC 与动态插件语义（与静态组件树取向冲突） |
| F-81 | 定时任务面板 | 依赖 schedule 域 |
| F-82 | 账号 | 依赖 DeepSeek 账号与计费链路 |

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
| 框架 | Dioxus `0.8.0-alpha.3`（alpha），框架 MSRV 为 Rust 1.85；实际下限由 `dioxus-cli` 决定：**Rust ≥ 1.93** + edition 2024 |
| 工具链 | 桌面目标需系统 WebView 开发库（Linux: WebKitGTK 4.1 + libsoup3）+ `dx` CLI；浏览器形态另需 `wasm32-unknown-unknown`。环境一致性用 `scripts/setup-env.sh` 校验 |
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
| M4 多 Agent 协作（P2 主线） | 子 Agent 目录/集群视图、子会话导航、续聊与独立停止、只读态、实时状态、列表隔离 | S9 跑通：父会话可浏览多个子 Agent，进入任一子会话并续聊/停止，普通侧栏不混入子会话 |
| M5 打磨 | 虚拟列表、长会话、重连鲁棒性、体积优化 | 达到 §7 指标 |
| P3 后续 | 原 P2 其余项 + F-71b + F-73/74 + F-80/81/82 | 不设退出标准，按需排期（见 §5.9） |

> M4 是本期唯一的功能主线。P1 收尾项（附件与 `@` 引用、会话搜索、Trajectory 用量/耗时聚合）不单独设里程碑，随 M4 并行推进；P3 项不占用本期排期。
>
> **进度（2026-11-03）**：
>
> - **M3 核心已实现**：三栏/拖拽调宽/窄屏规则、主题与字号字体持久化、完整设置页（General / Models / Agent presets / About）、搜索、置顶/fork、workspace 增删改与按钮排序、附件、`@`/斜杠命令、模型/权限选择、文件引用与公式、消息复制/重试/反馈、plan 卡片、文件树、Trajectory 聚合。F-15 排序为按钮式、F-16 为轻量状态（运行/等待/错误），完整 i18n 已移 P3；M3 的真机手工清单仍待跑。
> - **M4 的 P2-1～P2-4 已实现**：子 Agent 目录树、列表 / 关系图切换、独立关系图入口、地址化子会话导航、续聊 / 独立停止、只读态、`session/control` 实时状态、侧栏过滤，以及目录读取失败/迁移提示。**未完成**：真实模型/工具链路派生子 Agent 的端到端验证，以及 S9 手工清单，因此 M4 仍为进行中，DoD 的 S9 项保持未勾选。

---

## 10. 验收标准（DoD）

- [ ] 单一 Host（`dsh web`）可同时服务上游前端与 dsh-dioxus-ui，互不影响
- [ ] S1–S7 手工清单全部通过（每项附操作步骤与预期）
- [ ] S9 多 Agent 清单通过：目录展开、子会话导航、续聊、独立停止、只读态各一条用例
- [ ] 协议回放测试覆盖 P0 全部 endpoint 与帧类型，以及 P2 的 `subagents/prompt` / `subagents/interruptByParent` / `session/control`
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
| 子 Agent 目录依赖 Host projection | 旧 Host 无 `subagentCatalog`，入口不可用或层级缺失 | 目录缺失即隐藏入口；`migration-required` 时只允许打开历史，不展开子级；不猜测子会话关系 |

---

## 12. 协议事实核验

**已实测确认**（2026-11-03，`dsh transport/examples/probe.rs` 对源码形态 Host 跑通）：

| # | 结论 | 证据 |
| --- | --- | --- |
| 1 | `dsh web --no-open --port 0` 打印 `dsh web: http://127.0.0.1:<port>/?token=<43 字符>` | 解析成功，端口由 OS 分配（如 43621） |
| 2 | `GET /connect?token=…&interactions=answer` 返回的 cookie **可用于一元 `/api/*`** | `session/list` 调用成功 |
| 3 | `session/list` 的 `args` **必须是** `{"_request":{}}` | 传 `{"request":{}}` 被 `gateway/arguments-invalid` 拒绝：`missing "_request"; unexpected "request"` |
| 4 | `$events` 以 `{"args":{}}` 打开，首帧为 `ready`（含 `clientId` 与 `host.home`） | 收到 `clientId=92f9d4bf-… home=/home/raykwok` |

**仍待确认**：

1. 其余 endpoint 的 `args` 键集合（`commands/execute`、`workspaceFiles/read`、`fileUploads/upload`）——按同样方法逐个探测，或 dump `lib/typert.host.js` 的 descriptor。
2. `permessage-deflate` 协商行为（Host 默认 `deflate`；目前首帧正常，长帧未压测）。
3. 二进制结果（multipart）在 Rust 端的解析（按 part 名而非顺序）。
4. Web profile 下各 namespace 的实际可用方法集（缺失 provider 时的错误形态）。

终端已实测（`examples/terminal-probe.rs`）：`environment` 用 `{agentId}`、`create` 用 `{agentId, request}`、`attach` 用 `{agentId, id}`；`id` 客户端 mint，规则 `^[\w-]{1,128}$`。

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
| 子 Agent 目录/续聊 | `packages/client/ui-subagent/src/client/{SubagentHeaderLineage.tsx,SubagentReadOnlyComposer.tsx,index.ts,sidebar-chat/*}`、`packages/subagent/subagent/src/{projection-types.ts,control-types.ts,index.ts,list-children.ts}` |
| 子 Agent 设置 | `packages/client/ui-settings-subagent/src/client/*`（`maxDepth` / `maxActiveSubagents`、模型选择） |
| 架构文档 | `docs/api-gateway.md`、`docs/subsystems/web-client.md`、`docs/subsystems/slots.md`、`docs/web-styling.md` |

> Dioxus 侧依据见 `ARCH.md` §2。
