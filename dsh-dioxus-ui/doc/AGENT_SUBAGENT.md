# Agent 与 Subagent：关系、创建、管理与显示

| 项 | 内容 |
| --- | --- |
| 文档 | 设计参考（Agent / Subagent 子系统） |
| 项目 | `dsh-dioxus-ui` 与上游 `deepseek-harness` |
| 日期 | 2026-11-03 |
| 上游基线 | `deepseek-harness` @ `ce7073d512` |
| 关联 | `PR.md`（需求）、`ARCH.md` §10.4 / §13.1（客户端实现） |

本文回答五件事：

1. Agent 与 Subagent 是什么关系；
2. 子 Agent 如何被创建、如何管理；
3. 父子之间如何通信；
4. 父子关系如何被持久化、如何显示；
5. dsh 端（Host）需要怎样的包组合与构建步骤才能支撑这些能力。

> 结论先行：**Agent 是运行中的驱动者，Session 是持久日志，Subagent 是拥有独立
> Session 的子 Agent**。父子关系不靠内存指针维持，而靠父会话的
> `subagent/catalog`、子会话 header 的 `parentSession` / `origin=subagent` /
> `delegationDepth`，以及子会话自己的 `subagent/descriptor` 共同持久化。

---

## 1. 概念与关系

### 1.1 四个对象

| 对象 | 定义 | 生命周期 | 真源 |
| --- | --- | --- | --- |
| `Agent` | 驱动一个 Session 的运行时句柄（`ctx.agents`）；负责收发消息、跑 turn/step、调工具 | 进程内；可创建/恢复/销毁 | 运行时注册表 |
| `Session` | 仅追加的事件日志（`ctx.sessions`）；消息、turn、工具、descriptor 都在这里 | 持久化；跨重启 | Session 日志 |
| `Subagent` | 由另一个 Agent 通过 `ctx.subagents.startActivation()` 启动的子 Agent；拥有**自己的 Session** | 受 activation 管理 | 子 Session + 父目录 |
| `Activation` | 一次受管理的子级执行：预留身份/容量、创建或恢复 Agent、投递初始任务、结算并清理 | 一次运行 | `SubagentManager` |

关键点：

- **不是「一个 Agent 里跑多个子线程」**：每个子 Agent 有独立的 Session、独立上下文、
  独立工具作用域；父子之间只通过消息、完成通知与共享工作目录协作。
- **父子关系是数据，不是对象引用**：父 Agent 退出/重启后，仍能从持久日志枚举目录；
  但**冷恢复/续聊需要精确的父 Agent 在线授权**（否则报 `subagent/parent-unavailable`），
  授权对象是活的 Agent 实例，不是“持久化里的父 id”。
- **目录不继承**：普通 `session/fork` 会复制父日志前缀，但 `subagentCatalog`
  projection 会排除继承事实；fork 子级不会被当成父级的 subagent 成员。

### 1.2 关系图

```text
顶层 Agent (Session A, origin 无)
  │
  ├─ subagent/catalog 事件（A 的日志）
  │     ├─ child B: continuable  ──▶ 子 Agent (Session B)
  │     │                              ├─ subagent/descriptor（B 的日志）
  │     │                              ├─ subagent/catalog（B 的子级）
  │     │                              └─ subagentTiming / tokenUsage（B 的 projection）
  │     └─ child C: one-shot / external（无本地可续会话）
  │
  └─ subagent/catalog（A 的 projection）──▶ 客户端目录树 / list_agents
```

两套血缘同时存在：

| 方向 | 载体 | 用途 |
| --- | --- | --- |
| 父 → 子 | 父 Session 的 `subagent/catalog` 事件 / `subagentCatalog` projection | 目录枚举、UI 树、`list_agents`、归档准入 |
| 子 → 父 | 子 Session header 的 `parentSession`、`origin: "subagent"`、`delegationDepth` | 侧栏过滤、深度计算、恢复校验、活动汇总 |

### 1.3 子级形态

| 形态 | provider 示例 | `mode` | 有本地 Session | 可续聊 | 父级通信方式 | 说明 |
| --- | --- | --- | --- | --- | --- | --- |
| 本地 spawn | `spawn` | `continuable` | 是 | 是 | 内部 `ctx.subagents.sendMessage` / Agent inbox | 全新上下文；共享父进程的 Agent 工厂、LLM、工具服务 |
| 本地 fork | `fork` | `continuable` | 是 | 是 | 内部 `ctx.subagents.sendMessage` / Agent inbox | 用父级**已完成 turn** 作为 seed；不包含进行中的 turn |
| 外部执行 | `acp` | `external` | 否 | 否 | ACP（JSON-RPC over stdio），一次任务 | 进程隔离；只回传最终回答/诊断，中间流量不进入父会话 |
| 外部执行 | `codex` / `claude-code` / `dsh-sdk` | `external` | 否 | 否 | 各自进程/SDK 传输，一次任务 | 不实现 ACP，也不接受后续消息 |
| 历史 one-shot | 旧日志 / 无续聊能力的后端 | `one-shot` | 可能有 | 否 | 无（只读） | 只读记录；目录仍可发现 |

`mode` 的权威来源是子会话自己的 `subagent/descriptor`；父目录行也带 mode，
用于不打开子日志就能画 UI。`unknown` 表示 descriptor 缺失/损坏：仍可发现，
但不能续聊，打开后按只读处理。

### 1.4 三种边界：进程内 / 跨进程 / 跨 Host

按**执行边界**而不是 provider 名字分类，子 Agent 可以分成三类；上游目前只实现了前两类：

| 分类 | 上游实现 | 进程/主机 | 共享能力 | 子会话 | 可续聊 | 现状 |
| --- | --- | --- | --- | --- | --- | --- |
| 进程内 | `spawn` / `fork` | 同进程、同 Host | `ctx.agents`、`ctx.tools`、`ctx.llm`、工作目录、权限根 | 本地 durable Session | 是 | 默认能力 |
| 跨进程（进程外） | ACP / Codex / Claude Code / DSH SDK / OAR | 独立进程，**默认仍同一台机器** | 只传任务 + 本地工作目录 | 无本地 Session | 否，一次任务 | 已支持 |
| 跨 Host | 无 | 不同机器 | 需要远程工作区映射、远程会话/授权 | 远程 Session（未定义） | 未定义 | **未实现** |

术语纠正：

- 上游 ACP / DSH SDK README 里的 “remote child / remote process” 指**进程外**，
  不是跨机器。两者都明确写着 “local workspaces only”“remote runtimes would need
  their own backend”。
- `mode` 只表达 `one-shot` / `continuable` / `external`，**不编码主机边界**；
  `external` 是“进程外/无本地 Session”，不等于跨 Host。
- 把 ACP 命令包一层 `ssh` 理论上能让进程跑在另一台机器，但工作目录会变成远端路径，
  上游没有远程 workspace 映射、远程 session 续聊与跨机授权语义，因此不算受支持的能力。
- 浏览器远程访问同一个 Host 是“远程客户端”，不是跨 Host 子 Agent。

若要做跨 Host 子 Agent，需要新增一个 provider，并补齐：

1. 远程工作目录映射（父级 local path ↔ 远端 path）；
2. 远程 Session 身份、持久化与续聊所有权（是否允许 `continuable`）；
3. 跨机认证/审批与凭据信任边界；
4. 父级在线授权的分布式等价物（本地 `parent-unavailable` 语义如何跨机表达）；
5. 结果/诊断的传输与安全上限。

A2A 可以作为这类 provider 的传输协议，但当前基线没有 A2A 实现；不要把它与本地
Agent inbox 混为一谈。

### 1.5 持久血缘字段

子 Session header（`SessionWireHeader`）包含：

```rust
pub struct SessionWireHeader {
    pub id: SessionId,
    pub parent_session: Option<SessionId>,   // 直接父会话
    pub origin: Option<String>,              // "subagent"
    pub delegation_depth: Option<u32>,       // 递归深度，从 1 开始
    pub is_seeded: bool,                     // fork 的继承前缀标记
    pub agent_preset: Option<String>,
    // …
}
```

`session/list` 的 `SessionSummary` 只暴露 `parentSessionId` 与
`origin: "subagent"`；深度在 header 里，列表不返回。

---

## 2. 创建

### 2.1 模型侧：一次工具调用

默认 Web/Desktop preset 会挂两个委派工具：

| 工具名 | provider | 语义 |
| --- | --- | --- |
| `subagent` | `spawn` | 全新子 Agent，任务必须自包含 |
| `subagent_fork` | `fork` | 继承父级已完成 turn，适合接着当前工作做 |

模型调用时提供 `description`（短标签）与 `prompt`（任务）；可选 `cwd`、
`agent_preset`、以及（spawn 已启用时）`provider` / `model` / `reasoning_effort`。

工具**立即返回**类似：

```text
started subagent session-<child-id>
```

它不等待子任务完成。子任务由 activation 独立拥有；结束后父级收到一条完成通知
（最终回答 + 可选结构化结果 + 安全诊断）。

### 2.2 Host 内部流程

`SubagentManager` 的一次本地创建：

1. **解析深度**：`parent.delegation_depth + 1`，与工具的 `maxDepth`（或 Host
   `subagent.maxDepth`）比较；超限直接拒绝启动。
2. **预留容量**：从该 activation 树共享的池中占一个名额（`maxActiveSubagents`）；
   根父级不占名额。
3. **准备子级**：`prepareContinuable()` 决定 seed；spawn 返回空 seed，fork 返回
   父级已完成 turn 的日志前缀。
4. **创建 Session/Agent**：`ctx.agents.create()`，`meta` 写入
   `parentSession`、`origin: "subagent"`、`delegationDepth`、`cwd`、
   `agentPreset`、`isSeeded`。
5. **写父目录**：向父 Session 追加 `subagent/catalog` 事件（创建时只记成员关系，
   不记执行状态）。
6. **写子身份**：向子 Session 追加 `subagent/descriptor`（版本 3，模型不可见）：
   `mode`、`provider`、`label`，continuable 还记录模型路由、persona、toolFilter。
7. **投递初始任务**：把 prompt 放进子 Agent 的 inbox，启动 turn。
8. **结算**：子 Agent 活动、待处理输入与它拥有的后代都结束后，清理资源、归还容量，
   向父级发完成通知，然后 `dispose()`。

外部后端的差异：

- 不创建本地 Session/Agent，而是 `provider.start()` 返回一个 `SubagentRun`；
- `childId` 由后端分配，全局唯一；
- 只有 `delivery: "parent"` 时才向父目录追加 `subagent/catalog`，条目标
  `mode: "external"`；
- 天然不可续聊。

### 2.3 程序化创建

插件/工作流可以直接调用：

```ts
const activation = await ctx.subagents.startActivation({
  provider: 'spawn',
  label: 'review',
  delivery: 'parent',        // 或 'caller'：只把结果交给调用方，不发父级通知
  request: {
    prompt: [{ type: 'text', text: 'review the diff' }],
    cwd: '/path/to/repo',    // 可选；相对路径按父级当前目录解析
    maxDepth: 2,             // 可选；不得超过 Host/工具上限
    agentOptions: { model: '…' },
  },
  signal,
})
// activation.childId / activation.messageId / activation.result / activation.dispose()
```

`delivery: "caller"` 不向父级注入完成通知，适合 workflow/PTC 自己收集结果；
本地子级无论哪种 delivery 都会进入父目录。

### 2.4 浏览器不创建子 Agent

浏览器只能**续聊/停止**已经存在的 continuable 子会话，不能新建子 Agent。
新建入口只有两个：模型调用委派工具，或 Host 侧插件直接调 `startActivation()`。
这是权限设计：创建需要 Agent 身份与父级授权，浏览器只持有父子地址。

---

## 3. 管理

### 3.1 深度：`maxDepth`

- Host 设置：`subagent.maxDepth`，默认 `1`（只允许直接子级）；`0` 禁止委派。
- 工具配置：`tool-subagent.config.maxDepth`，可覆盖 Host 设置；`'provider-managed'`
  表示不向进程外后端传上限。
- 每次启动都重新读取；修改在下一次委派生效。
- 深度是**持久化**的：子 header 的 `delegationDepth` 是单调下限，恢复后的父级
  不能假装自己是顶层再往下派。

### 3.2 容量：`maxActiveSubagents`

- Host 设置项，默认 `8`；限制**每棵 activation 树**中在线 + 正在创建的子级数。
- 本地与外部后代共享同一个池；根父级不占名额。
- 等待后代、有待处理 inbox、正在停止的 activation 仍计入容量；向已驻留子级发消息
  复用其名额。
- 满额时新建/冷恢复返回 `ACTIVATION_LIMIT_REACHED`；浏览器消息返回
  `subagent/delivery-unavailable`。不排队。
- 调高上限后已有树可继续接纳；调低后存量子级继续运行，降到上限以下前拒绝新接纳。

### 3.3 消息与中断权限

| 操作 | 谁可以做 | 规则 |
| --- | --- | --- |
| `send_message` | 任何**确切在线**的 Agent | 只能发给自己的直接 continuable 子级；驻留的 continuable 子级可发给自己的直接父级 |
| `interrupt_agent` | 在线父级 / 祖级 | 只停目标当前 turn；队列保留；后代继续跑；自身、同级、陈旧调用方被拒绝 |
| 浏览器续聊 | 人类客户端 | `subagents/prompt` 必须携带精确 `parentSessionId + childSessionId + mode`，父不在线则拒绝 |
| 浏览器停止 | 人类客户端 | `subagents/interruptByParent`，同样按父地址授权 |

模型工具 `send_message` / `interrupt_agent` / `list_agents` 由
`@deepseek-ai/dsh-tool-subagent-control` 注册；它们只是
`ctx.subagents` 的薄适配器，不自己维护生命周期。消息进哪个 inbox、消息长什么样、
父子各自能看到什么，见 §4。

### 3.4 生命周期与清理

- activation 的 `dispose()` 会停止该次执行并等待其后代（含正在启动的后代）清理；
- 本地子级失败且没有成功提交 `turn/end` 时，等它拥有的后代结束后以 `error` 结算；
- 父级归档/停止时，Workspace 的 archive admission 通过
  `workspace/session-activity` 找到运行中的 subagent 后代，逐个取消；一个子级拒绝
  取消只记日志，兄弟继续停；
- 清理失败不会覆盖已捕获的结果：结果与父通知保留，`dispose()` 拒绝，
  `subagent/end` 标 `error`。

### 3.5 冷恢复

continuable 子级在空闲/重启后可通过 `subagents/prompt` 或 `send_message` 冷恢复：

- 需要精确的父 Agent **在线**并提供授权（冷恢复不是“凭持久化父 id 就能恢复”）；
- 需要持久化 + Session 查询服务（`session-persistence-jsonl` +
  `session-query-sqlite`）；
- 用子会话自己的 `subagent/descriptor` 重建 composer（模型路由、persona、
  toolFilter），不依赖原 provider 仍然注册；
- 如果子级独立选择了 preset，首条自有 descriptor 之前会有
  `agent-preset/selected`；恢复时按这个顺序解析；
- one-shot / external 不能冷恢复。

### 3.6 用户设置

Web/Desktop 的 **Plugins → Subagent** 页面（`ui-settings-subagent`）编辑两个
namespace：

| namespace | 字段 | 作用 |
| --- | --- | --- |
| `subagent` | `maxDepth`、`maxActiveSubagents` | 委派深度与并行容量 |
| `subagent-model-selection` | 允许子级使用的 provider/model route 列表 | 子级模型选择 |

用户设置覆盖插件组合里的默认值；恢复默认会删除用户覆盖。修改在一次新的委派上生效。

### 3.7 关系模型：树，不是 DAG

子 Agent 的管理**不是 DAG 调度**，而是“父子目录树 + 每棵 activation 树的容量池”：

- 每个子 Session 只有一个 `parentSession`；每条 `subagent/catalog` 记录只属于一个
  父会话。父子边唯一，因此结构是**有向树/森林**，不是 DAG。
- 消息权限按**直接相邻边**授权（父↔直接子），archive/清理按树向下，深度单调递增；
  没有拓扑排序、依赖合并、多父节点或跨分支调度。
- 一个根会话及其所有后代构成一棵 activation 树；容量池按这棵树共享，根父级不占名额。
- DAG 出现在上游的其它子系统：`dsh-experimental-agent-team` 的 task board 用
  task 依赖 DAG；`workflow-ptc` 的工作流也有 DAG 语义。它们管理的是“任务/步骤依赖”，
  不是 subagent 的父子所有权。

所以 UI 上可以把子 Agent 图叫“DAG 图”，但以当前数据模型渲染出来一定是树/森林；
只有未来引入多父（team 任务、A2A 跨 Agent 引用）才需要真正的 DAG 布局。

---

## 4. 通信

### 4.1 通信路径总览

子 Agent 与父 Agent **不共享上下文、不共享 turn、不共享 transcript**；它们共享进程、
工作目录、权限根与工具服务，通信只有下表几条显式路径。本节描述**进程内
spawn/fork 子级**以及浏览器对本地子级的控制；ACP 等外部后端的边界见 §4.7。

| 路径 | 入口 | 边界 | 归属 |
| --- | --- | --- | --- |
| 父模型 → 子初始任务 | `subagent` / `subagent_fork` 工具 | 子 inbox 的 `next-turn`（queue） | 子会话 user 消息，`source: { kind: "user" }` |
| 父模型 → 子后续消息 | 模型工具 `send_message` | 子 inbox 最近 step（steer） | 子会话 user 消息，`source.kind = "agent-message"` |
| 子 → 父主动汇报 | 模型工具 `send_message` | 父 inbox 最近 step（steer） | 父会话 user 消息，`source.kind = "agent-message"` |
| 子完成 → 父通知 | 运行时自动 | 父空闲时 `next-turn`，父忙时最近 step | 父会话 user 消息，`source.kind = "subagent-settled"` |
| 浏览器 → 子续聊 | `subagents/prompt` | 子 inbox：`queue` 或 `steer` | 子会话 user 消息，`source.kind = "user"` + `rpcId` |
| 浏览器/模型 → 停止 | `subagents/interruptByParent` / `interrupt_agent` | 目标当前 turn | 不改写历史，只发 cancel |
| 发现 | `list_agents`、`subagentCatalog` projection | 只读 | 不产生消息 |

权限是**相邻边**：父只能发给直接 continuable 子级；只有驻留（resident）的
continuable 子级能发给自己的直接父级。兄弟、祖父、任意跳级都不在授权范围。
`dsh-experimental-agent-team` 的任意成员互发是另一套实验能力，不属于 subagent 通信。

### 4.2 两个 inbox 边界：queue 与 steer

Agent 的 pending 输入分两个位置：

| 位置 | 语义 | 典型来源 |
| --- | --- | --- |
| `next-turn`（queue） | 当前 turn 结束后，作为下一条独立用户消息开启新 turn | 父级初始任务、浏览器选择“排队”、父空闲时的完成通知 |
| `next-step`（steer） | 当前 turn 内、最近一个 step 边界插入 | `send_message`、浏览器选择“插话”、父忙碌时的完成通知 |

- 两者都是 FIFO；接受后返回稳定的 `messageId`。`send_message` 只保证 inbox 接受，
  不等待模型处理或回复。
- `steer` 不打断当前正在执行的 step，只影响下一个 step；真正停止用 interrupt。
- interrupt **不清空队列**：未认领的 pending 消息保留，等下一次 waking send 恢复。
- 父级忙碌时收到完成通知会走 `steer`，不会立刻发起新的模型请求；父级空闲时走
  `queue`，会开启一个独立 turn 让父模型阅读结果。
- 目标不驻留时：本地 continuable 子级走冷恢复（需要父在线 + 持久化/查询服务）；
  external 拒绝后续消息；one-shot/unknown 不可恢复。

### 4.3 消息形状与归属

**Agent 消息**（父 → 子、子 → 父）：

```text
Agent <sender-session-id> sent a message: <content>
```

```json
{
  "source": {
    "kind": "agent-message",
    "form": "relay",
    "senderSessionId": "<sender>"
  }
}
```

子 Agent 的首条任务里会附带父 id 与返回指引（只要子级可用 `send_message`，
且本次是 parent 投递）：

```text
Your parent agent id is "<parent-id>". Before you finish, send your result to that
agent with send_message({ agent_id: "<parent-id>", message: "<self-contained result>" }).
...
sending a message does not end your turn.
```

因此子级可以在工作中多次汇报，发消息不会结束当前 turn；父子共享工作区，但父级
**不会自动收到**子级的 transcript、工具输出、推理过程。

**完成通知**（运行时生成）：

```json
{
  "source": {
    "kind": "subagent-settled",
    "form": "notice",
    "summary": "Background subagent <child-id> finished …",
    "senderSessionId": "<child-id>"
  }
}
```

正文由四部分组成：

1. 一行结算摘要（completed / aborted / max-tokens / refusal / error）；
2. 子级最后一条非空 assistant 文本（没有则 `It left no closing message.`）；
3. 可选 `Structured result: <JSON>`；
4. 可选 provider 安全诊断。

如果子级在结束前刚发过 `send_message`，完成通知会省略 closing text，并说明
“它刚发过消息”；避免同一段结果在父会话里出现两次。

以上完成通知只在 `delivery: "parent"` 时发送。workflow / PTC 使用
`delivery: "caller"` 时，结果通过 `activation.result` 交给调用方，不注入父会话；
本地子级仍然进入父目录，之后仍可被发现/续聊。

**浏览器 prompt**：

```json
{
  "source": {
    "kind": "user",
    "rpcId": "<requestId>",
    "clientTimeZone": "Asia/Shanghai",
    "principal": "<非 owner 调用方时才写>"
  }
}
```

它在子会话里就是一条普通用户消息；`subagents/prompt` 会先做图片附件准入，再进
子 inbox。父 Agent 必须在线，否则返回 `subagent/parent-unavailable`。

### 4.4 一次典型协作的时序

```text
父模型 ── subagent(description, prompt) ──▶ Host
Host   ── 创建子 Session + descriptor/catalog ──▶ 子 inbox: 初始任务（queue）
子     ── turn/start …（共享工作目录，独立上下文）
子模型 ── send_message(parent_id, "先报告一个发现") ──▶ Host
Host   ── Agent <child> sent a message: …（steer）──▶ 父 inbox
父     ── 在最近 step 读到子级消息（不新开 turn）
子     ── turn/end(completed)
Host   ── 完成通知（父空闲 queue / 父忙碌 steer）──▶ 父 inbox
父模型 ── 读取结果，决定继续委派 / 自己收尾
```

浏览器续聊走同一条子 inbox，只是来源换成 `user`：

```text
浏览器 ── subagents/prompt(delivery=queue|steer) ──▶ 子 inbox
子     ── 运行；结束后仍按父级投递规则发完成通知
浏览器 ── subagents/interruptByParent ──▶ 只停当前 turn（不清队列）
```

### 4.5 中断、取消与关闭的通信含义

| 动作 | 子级 | 父级 | 队列/历史 |
| --- | --- | --- | --- |
| `interrupt_agent` / `interruptByParent` | 停当前 turn；external 永久停 | 不受影响 | 未认领消息保留；已认领不重放 |
| 父 turn 被 cancel（`session/cancel`） | **不受影响**（子级是独立 Agent） | 当前 turn 停止 | 子级继续运行；要停子级需显式 interrupt / archive |
| 父归档 | archive admission 逐个取消运行中的后代 | 父级不再接纳新子级 | 一个子级拒绝取消只记日志，兄弟继续停 |
| 父 Agent dispose / 宿主停止 | 关闭准入并 drain 后代，等待清理 | 归还容量 | 迟到工作被拒绝 |

`interruptByParent` 特意**不查找父 Agent**：只要地址能对上一个 live Activation，
父级离线时仍可停止子级。需要父级在线的只有 `subagents/prompt`（续聊）。

### 4.6 客户端如何观察通信

| 通道 | 看到什么 | 用途 |
| --- | --- | --- |
| 子会话 `session/follow` | 子级 user/assistant/tool 事件、descriptor、目录事件 | 打开子会话、Trajectory |
| 父会话 `session/follow` | 父级收到的 `agent-message` / `subagent-settled` user 消息、父级自己的事件 | 父会话消息流 |
| `session/control` | `subagentCatalog` / `subagentTiming` / `subagent` / `tokenUsage` | 目录、状态、耗时、用量（无需打开子日志） |
| `$events` waterfall | 子级工具触发的审批/提问，`agentId` 是子会话 id | 审批/提问卡片 |
| `subagents/*` 一元 RPC | prompt / interrupt 回执 | 浏览器主动操作 |

上游聊天页在 `agent-message` / `subagent-settled` 开启新 turn 时渲染
“Turn trigger”（子代理消息 / 子代理完成）；以 `steer` 进入当前 turn 时按上下文
消息处理。本项目当前 `map::rows` 只读文本，不渲染 source 标签，因此这些消息会以
普通用户消息出现。审批/提问按 `agentId` 路由也尚未在 UI 层区分父子会话，属于
已知显示差距。

### 4.7 与 ACP / A2A 的边界

**结论**：父子日常通信走 Agent inbox，**不是 ACP，也不是 A2A**。ACP 只出现在
“Host 作为客户端去驱动一个外部 ACP agent”的场景；当前基线没有 A2A 实现。

| 链路 | 协议 | 谁发起 | 是否可续聊 |
| --- | --- | --- | --- |
| Host ↔ 进程内 spawn/fork 子级 | `ctx.subagents.sendMessage` + Agent inbox（进程内 durable user message） | 父/子 Agent | 是（continuable） |
| Host ↔ ACP 子级 | ACP v1：JSON-RPC over stdio（握手 + 新建会话 + 任务提交） | Host 作为 ACP client | 否，每次一个 task |
| Host ↔ Codex / Claude Code / DSH SDK 子级 | 各自进程/SDK 传输 | Host | 否 |
| 浏览器 ↔ Host | dsh HTTP RPC + WebSocket mux（`session/follow`、`subagents/prompt` …） | 浏览器客户端 | 不适用 |
| A2A | 当前基线未实现 | — | — |

为什么本地子级不用 ACP：

- 进程内子 Agent 与父级共享 `ctx.agents`、`ctx.tools`、`ctx.llm`；消息就是往对方
  Session 写一条 durable user message 并唤醒/steer，没有序列化边界。也正因此才支持
  queue/steer、父子双向 `send_message` 与热恢复（resident 时）。
- ACP 是**进程边界**：子 agent 有自己的 runtime、session、model、tools；父级只传
  任务与工作目录，经 ACP 拿回最终文本或安全诊断，中间消息和工具流量留在子进程。
  它在目录里表现为 `mode: "external"`，不能打开本地子会话，也不能后续消息。
- A2A（Agent2Agent）在当前 `deepseek-harness` 基线里没有包或协议实现。若未来接入，
  它应当作为**外部 provider 的传输**（像 ACP 一样）建模，而不是替换本地 Agent
  inbox；`dsh-experimental-agent-team` 的成员互发是同进程实验能力，也不是 A2A。

因此三种“通信”不要混用：

| 说法 | 实际含义 |
| --- | --- |
| 父子通信 | Agent inbox / durable user message（内部协议） |
| 委派到 ACP 子 agent | Host 通过 ACP 驱动外部进程，执行一次任务 |
| 浏览器控制子会话 | 浏览器通过 dsh gateway 调 `subagents/prompt` / `interruptByParent` |

---

## 5. 数据与协议

### 5.1 事件

| 事件 | 写入方 | 内容 | 用途 |
| --- | --- | --- | --- |
| `subagent/catalog` | 父 Session | `{ version, childId, childCreatedAt, mode, label? }` | 父目录成员关系；创建时写一次，不随状态更新 |
| `subagent/descriptor` | 子 Session | `{ version, mode, provider, label, agentProvider?, agentModel?, agentReasoningEffort?, persona?, toolFilter? }` | 子身份与续聊能力；模型不可见，跨 compaction 保留 |
| `agent-preset/selected` | 子 Session | `{ agentPreset }` | 独立 preset 的恢复顺序 |
| `subagent/start` / `subagent/end` | Host 事件总线 | `SubagentRunInfo` / `SubagentRunEndInfo` | 运行时生命周期观测（非 durable 会话事件） |

### 5.2 projection

| key | 所在 Session | 形状 | 说明 |
| --- | --- | --- | --- |
| `subagentCatalog` | 父 | `[{ id, createdAt, mode, label? }]` | 直接子级，按父日志事件顺序；排除 fork 继承事实 |
| `subagentTiming` | 子 | `{ settledMs, active?: { since, through }, lastTurnCompleted? }` | descriptor 之后已闭合 turn 的累计 + 当前 open turn |
| `subagent` | 子 | `{ mode, label?, seq } \| null` | 子身份；null 表示没有有效 descriptor |
| `tokenUsage` | 子 | `{ uncachedInputTokens, outputTokens, cacheReadTokens, cacheWriteTokens }` | 四个互斥桶，供目录行显示用量 |

实测（2026-11-03）：`session/control` 的 baseline 同时携带上述四个键；无子会话时
`subagentCatalog` 是 `[]`，`subagentTiming` 是 `{"settledMs":0}`，`subagent` 是
`null`。

### 5.3 RPC / stream

| endpoint | args | 用途 |
| --- | --- | --- |
| `session/follow` | `{ request: { address } }` | `address` 为 `session` 或 `subagent`；子会话必须带 parent + mode |
| `session/control` | `{}` | 全 Host projection baseline + `projection` 增量（实时状态来源） |
| `session/projections` | `{ request: { sessionId } }` | 按需读取目录（展开子级 / 重试） |
| `subagents/prompt` | `{ request: { requestId, parentSessionId, childSessionId, mode: "continuable", delivery: "queue"\|"steer", content, clientTimeZone? } }` | 浏览器续聊 |
| `subagents/interruptByParent` | `{ childSessionId, parentSessionId, mode: "continuable" }` | 浏览器独立停止 |

`subagents/prompt` 的业务错误：`subagent/parent-unavailable`、`not-resumable`、
`unauthorized`、`delivery-unavailable`、`attachment-invalid`、`invalid-time-zone`。
对不存在的 child，实测返回 `subagent/not-resumable`；`interruptByParent` 对不存在
的 child 返回 `{ accepted: true }`（幂等），但真实子级是否停止仍以流事件为准。

### 5.4 地址

```rust
pub enum SessionAddress {
    Session { session_id },
    Subagent {
        parent_session_id,
        child_session_id,
        mode: /* one-shot | continuable | unknown */,
    },
}
```

- `session/follow`、`session/page` 都接受这个地址；
- 本项目不为子会话新增 URL 路由，地址放在客户端会话状态里；
- 浏览器续聊/停止用平铺的 parent/child 字段，不用 `SessionAddress` 外壳。

---

## 6. 显示

### 6.1 上游 `ui-subagent`

- 位置：会话 header 的 lineage/actions 槽位；
- 触发：父会话 `subagentCatalog` 非空（或读取失败）时出现“后代计数”按钮；
  空目录隐藏；
- 目录行：模式、状态点/加载环、标题、累计 token、当前 turn 耗时；可递归展开；
- 点击行：用精确地址打开子会话；行尾箭头：在右栏侧开；
- 输入区：continuable + 父在线 → 普通 composer（发送走 `subagents/prompt`）；
  one-shot / unknown / 父不可用 → 只读说明；父离线但运行中 → 保留停止；
- 普通侧栏按 `origin === "subagent"` 过滤，不展示子会话；
- `@` 引用源只列出运行中的子级，插入字面 `@label`，不授予续聊权限。

### 6.2 本项目 `dsh-dioxus-ui`

| 层 | 文件 | 内容 |
| --- | --- | --- |
| 协议 | `crates/proto/src/subagent.rs`、`session.rs` | 目录/计时/身份/续聊类型；`SessionAddress`；`SessionControlFrame` |
| 运行时 | `crates/runtime/src/lib.rs` | `follow_address` / `page_before_address` / `prompt_subagent` / `interrupt_subagent` / `refresh_projections` / `open_control` |
| 状态 | `crates/state/src/projection.rs` | `ProjectionStore`：control baseline 整窗替换 + projection 增量 |
| 视图 | `crates/ui/src/subagent.rs` | `SubagentCatalog` 触发器 + 列表/关系图切换 + `SubagentAside` |
| 会话页 | `crates/ui/src/conversation.rs` | 面包屑、只读 composer、目录挂载 |
| 装配 | `crates/app/src/main.rs`、`map.rs` | 地址导航、续聊/停止、目录刷新、侧栏过滤、右栏侧开 |

数据流：

```text
session/control ──baseline/projection──▶ ProjectionStore
                                             │
父会话 subagentCatalog ──────────────────────┼─▶ map::catalog_rows ──▶ SubagentCatalog
子会话 subagentTiming / tokenUsage ──────────┘

点击行 ──▶ SessionAddress::subagent ──▶ DshClient::follow_address ──▶ SessionView ──▶ 会话页
输入 ──▶ subagents/prompt（continuable）
停止 ──▶ subagents/interruptByParent
```

调试：`DSH_FAKE_SUBAGENT=1` 会注入一棵假目录（worker / reviewer / external），
用于无模型时验证目录、耗时、用量与只读态；`DSH_FAKE_SUBAGENT` 不会进入正常路径。

### 6.3 子 Agent 图（可选增强）

上游没有图视图，只有 header 里的缩进目录。本项目现在在同一个菜单里提供
**“列表 / 关系图”切换**：列表是默认且保持原样；关系图是只读的 tidy-tree，
数据足够，但它仍是树/森林布局，不是真正的 DAG 布局。

当前实现（2026-11-03）：

- `SubagentCatalog` 菜单头增加「列表 / 关系图」分段按钮，默认列表；
- header 的「子 Agent N」触发器旁边另有一个关系图图标按钮，点击直接以关系图打开；
- 关系图渲染在同一浮层内，水平边界与会话面板一致（跟随 `--shell-sidebar` /
  `--shell-rightbar`，不压住左侧栏或右栏），不改变列表；
- 节点显示：状态点 + label + 状态文本 + mode + 耗时/token；完整 session id 在
  节点 `title` 里；
- 已加载的层级全部画出来，不受列表折叠状态影响；
- 未加载子级的节点右上角有 `+`，点击走 `on_toggle` → `session/projections`，
  加载后自动出现在图上；
- 外部节点虚线/降透明度且不可打开；节点点击打开子会话，箭头按钮右栏侧开。

#### 节点与边

| 字段 | 来源 | 展示 |
| --- | --- | --- |
| `id` | catalog `id` | 节点标识（可缩短显示，title 保留全 id） |
| `parentId` | 当前目录的父会话 | 边的起点 |
| `label` | catalog `label`，否则 session id | 节点标题 |
| `mode` | catalog `mode` | 类型徽标：continuable / one-shot / external / unknown |
| `state` | `subagentTiming.active` + `lastTurnCompleted` | 运行 / 完成 / 空闲 状态点 |
| `elapsedMs` | `subagentTiming` | 本轮/累计耗时 |
| `tokens` | `tokenUsage` 四桶之和 | 用量 |
| `depth` | 递归展开时累加 | 布局层级 |
| `hasChildren` | 该节点目录是否已加载/非空 | 展开箭头 |
| `diagnostic` | `session/projections` 失败 / migration-required | 红色/警告节点 |

边只有一种：`parentSessionId → childSessionId`，来自父目录 `subagentCatalog`。
一个子节点只有一个父节点，所以画出来是树/森林；不要用环形或力导向布局制造
“多父/互相引用”的错觉。

```json
{
  "id": "session-child",
  "parentId": "session-parent",
  "label": "reviewer",
  "mode": "continuable",
  "state": "running",
  "elapsedMs": 6200,
  "tokens": 2340,
  "depth": 1,
  "hasChildren": false,
  "diagnostic": null
}
```

#### 数据加载

1. 当前会话的 `subagentCatalog` 来自 `ProjectionStore`（`session/control` 实时更新）。
2. 展开一个节点时，对它调用 `session/projections`（`{request:{sessionId}}`），
   读取该节点的 `subagentCatalog`、`subagentTiming`、`tokenUsage`；成功后写入缓存。
3. 节点状态/耗时/用量随 `session/control` 的 `projection` 增量更新；成员关系只在
   目录变化时重算。
4. 点击节点仍用 `SessionAddress::subagent` 打开；箭头仍在右栏侧开。
5. 没有“一次性取全树”的浏览器 RPC；图必须按需展开，不能假装一次拉全。

#### 布局与渲染

- 推荐 **tidy tree**（Reingold–Tilford）：`y = depth * 行高`，`x` 由叶子顺序回推；
  多棵树并排成森林。
- 用 SVG 画边与节点（`<path>` + `<g>`），或 HTML 节点 + SVG 边覆盖层；不要引入
  force-directed，节点移动会导致阅读位置丢失。
- 视觉编码：
  - 实线边 = 直接父子；虚线 = external 叶子（无本地 Session）；
  - 节点左侧状态点，右侧 mode 徽标 + 耗时 + token；
  - unknown 节点用警告色；读取失败显示诊断节点，不静默丢分支；
  - 运行中节点保留统一 loading，不用动画改变布局尺寸。
- 交互：点击打开、箭头侧开、展开/收起、hover tooltip、键盘上下/左右、fit/zoom；
  图的拖拽只做平移，不做“改父级”——父子关系由 Host 独占。

#### 性能与边界

| 约束 | 做法 |
| --- | --- |
| 目录可能很深 | 默认只展开 2 层；按需加载；单次最多渲染 200 个节点 |
| 状态更新频繁 | 合并 `projection` 帧，状态/数字节流（~500ms）；成员变化才重新布局 |
| 节点尺寸变化 | 固定节点宽高，label 省略 + tooltip；耗时/token 用等宽数字 |
| 环形/重复 id | 递归时维护 `visited`，重复 id 停止并标 diagnostic（当前协议不应出现） |
| migration-required | 节点标“需要迁移”，只允许打开历史，不展开子级 |

这是**客户端增强**，不改变 Host 协议，也不影响与上游的行为等价性。若未来要画真正的
DAG（任务依赖、多父、跨 Agent 引用），数据源应是 `agentTeam` task board 或 workflow
图，而不是 `subagentCatalog`。

---

## 7. dsh 端如何构建

### 7.1 分层

上游默认组合分两层：

| 层 | 文件 | 内容 |
| --- | --- | --- |
| base（Host 平面） | `packages/bundle/base/cordis.patch.yml` | `dsh-subagent` 服务、`spawn`/`fork` 后端、`session-projection`、持久化、`session-query`、会话日志 |
| web-app（客户端平面） | `packages/bundle/web-app/cordis.patch.yml` | `ui-subagent`、`ui-settings-subagent`、api-proxy；**禁用** Host 平面的委派工具 |
| preset（Agent 组合） | `packages/bundle/web-app/presets/{standard,ptc}.patch.yml` | 每个 Agent 的 `delegation` 组：`tool-subagent-control` + `tool-subagent` + `subagent_fork` |

为什么服务/后端留在 base：`ctx.subagents` 是**进程单例**，有跨会话的
`listChildren` / 后续消息查询面；provider 名字也要求全局唯一。preset 只决定
“这个 Agent 能看到哪些委派工具”。

### 7.2 最小组合（自定义 profile）

```yaml
- name: '@deepseek-ai/dsh-session-projection'   # base 已有；独立组合需显式挂
- name: '@deepseek-ai/dsh-subagent'
- name: '@deepseek-ai/dsh-subagent-spawn-in-process'
  config:
    providerName: spawn
- name: '@deepseek-ai/dsh-tool-subagent'
  config:
    provider: spawn
    toolName: subagent
- name: '@deepseek-ai/dsh-tool-subagent-control'
```

需要 fork 再加：

```yaml
- name: '@deepseek-ai/dsh-subagent-fork-in-process'
  config:
    providerName: fork
- name: '@deepseek-ai/dsh-tool-subagent'
  config:
    provider: fork
    toolName: subagent_fork
```

冷恢复另需 durable persistence 与 Session 查询服务（默认 base 已含）：

```yaml
- name: '@deepseek-ai/dsh-session-persistence-jsonl'
- name: '@deepseek-ai/dsh-session-query-sqlite'
```

### 7.3 关键配置

| 位置 | 字段 | 默认 | 说明 |
| --- | --- | --- | --- |
| Host 设置 `subagent` | `maxDepth` | `1` | 委派深度上限；`0` 禁止 |
| Host 设置 `subagent` | `maxActiveSubagents` | `8` | 每棵 activation 树容量 |
| `dsh-tool-subagent` | `provider` | 必填 | `spawn` / `fork` / 外部 provider |
| `dsh-tool-subagent` | `toolName` | `subagent` | 每个实例必须不同 |
| `dsh-tool-subagent` | `modelSelection` | `false` | 公开子级路由字段 |
| `dsh-tool-subagent` | `modelDiscovery` | `false` | 注册 `list_subagent_models` |
| `dsh-tool-subagent` | `agentPreset` | — | 子级独立 preset 默认值 |
| `dsh-tool-subagent` | `allowedAgentPresets` | — | 模型可选 preset 白名单 |
| `dsh-tool-subagent` | `maxDepth` | Host 设置 | 工具级覆盖；`'provider-managed'` 交给外部后端 |

Web/Desktop 的 `standard` 与 `ptc` preset 都为 spawn 开了
`modelSelection: true` 与 `modelDiscovery: true`，fork 保持关闭以复用父级前缀缓存。

### 7.4 源码形态联调（本项目）

`deepseek-harness` 源码不能直接 `pnpm install` 后跑 `dsh web`，还需要：

1. **Typert 描述符**：`lib/typert.host.js` 是构建产物；缺失时启动报
   `typert contributor(s) failed to register`。
2. **前端 client bundle**：`dsh web` 默认要求 63 个 `lib/client.js`。Rust 前端
   不加载它们，用 patch 关闭 `modules` 插件即可跳过整条前端构建链。

```bash
# 1) 依赖（首次）
corepack enable pnpm
corepack pnpm install

# 2) Host 描述符（约 4 秒；整仓 build 会一并生成）
pnpm exec tsx scripts/generate-typert.ts

# 3) 启动 Host（源码形态）
cat >/tmp/dsh-patch.yml <<'YAML'
- id: modules
  disabled: true
YAML

node --import tsx/esm apps/cli/src/bin.ts web \
  --patch /tmp/dsh-patch.yml \
  --no-open --port 0
```

`--patch` 必须紧跟 `web` 子命令，放在 `--port` 之后会被判为 unknown option。

本项目的 `scripts/dev-host.sh` 封装了以上步骤：

```bash
DSH_BIN=./scripts/dev-host.sh ./target/debug/dsh-dioxus-ui
```

### 7.5 生产构建

```bash
# 完整构建（Host + 前端 client bundles）
pnpm run build

# 只构建 Web 前端产物
pnpm run build:web
```

构建完成后，`dsh web` 自带完整前端与 `lib/typert.host.js`，不再需要上面的 patch；
`dsh-dioxus-ui` 仍以 `dsh web --no-open --port 0` 拉起 Host。

### 7.6 验证

```bash
# 协议形状（需要已有 Host 的 base/token）
DSH_BASE=http://127.0.0.1:3080/ DSH_TOKEN=… \
  cargo run -p dsh-runtime --example subagent-probe

# 预期：
#   control baseline 包含 subagentCatalog / subagentTiming / subagent / tokenUsage
#   session/projections 的 args 是 { request: { sessionId } }
#   subagents/prompt 对不存在的 child 返回 subagent/not-resumable
#   subagents/interruptByParent 返回 { accepted: true }
```

UI 侧无模型时：

```bash
DSH_FAKE_SUBAGENT=1 DSH_BIN=./scripts/dev-host.sh ./target/debug/dsh-dioxus-ui
```

打开会话后点击 header 的“子 Agent”触发器，应看到 worker（continuable）、
reviewer（one-shot）、external 三行及耗时/用量。

关系图视觉验收（debug 构建自动打开关系图，便于截图）：

```bash
DSH_FAKE_SUBAGENT=1 DSH_DEBUG_SUBAGENT_GRAPH=1 \
  DSH_BIN=./scripts/dev-host.sh ./target/debug/dsh-dioxus-ui
```

也可以不设 `DSH_DEBUG_SUBAGENT_GRAPH`，点击 header 触发器右侧的图图标直接进入；
`DSH_DEBUG_SUBAGENT_GRAPH` 只在 `debug_assertions` 下生效，不影响 release。

---

## 8. 已知限制

- 浏览器只能续聊/停止，不能创建子 Agent；创建权限属于模型工具或 Host 插件。
- `external` 子级没有本地 Session，不能打开、不能续聊；UI 只显示为叶子。
- `one-shot` / `unknown` 只能看历史，不能续聊；`unknown` 仍可被目录发现。
- 目录只记录创建时的成员关系，不携带执行状态；状态来自 `subagentTiming` /
  在线 Agent 注册表。
- 普通 `session/fork` 不是 subagent：不会进入目录，也不会被 `list_agents` 发现。
- 容量只存在于当前进程，不限制累计 Session 历史或 token 用量。
- 上游在目录读取失败时仍显示入口；本项目当前只在目录非空时显示，读取失败会隐藏
  （后续补错误态）。
- 本项目尚未跑通“真实模型派生子 Agent”的端到端（需要可用模型/工具链路）；
  S9 手工清单中的续聊/停止仍以待真机验证。

---

## 9. 上游对照

| 主题 | 文件 |
| --- | --- |
| 子 Agent 契约 | `packages/subagent/subagent/src/{index,manager,activation,child-agent,list-children}.ts` |
| 目录事件与 projection | `packages/subagent/subagent/src/{catalog,projection,projection-types}.ts` |
| 子身份 descriptor | `packages/subagent/subagent/src/descriptor.ts` |
| 浏览器控制面 | `packages/subagent/subagent/src/{control,control-types}.ts` |
| 委派工具 | `packages/subagent/tool-subagent/src/index.ts` |
| 管理/发现工具 | `packages/subagent/tool-subagent-control/src/{index,list-agents}.ts` |
| 本地后端 | `packages/subagent/subagent-spawn-in-process/`、`subagent-fork-in-process/` |
| 外部后端 | `packages/subagent/subagent-{acp,codex,claude-code,dsh-sdk}/` |
| Host 组合 | `packages/bundle/base/cordis.patch.yml`（§ subagent） |
| Web 组合 | `packages/bundle/web-app/cordis.patch.yml`、`presets/{standard,ptc}.patch.yml` |
| 客户端目录 | `packages/client/ui-subagent/src/client/SubagentHeaderLineage.tsx` |
| 客户端只读 composer | `packages/client/ui-subagent/src/client/SubagentReadOnlyComposer.tsx` |
| 客户端设置 | `packages/client/ui-settings-subagent/src/client/*` |
| 子系统文档 | `docs/subsystems/subagent.zh.md` |
