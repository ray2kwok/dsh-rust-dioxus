# AGENTS.md

本文件用于向 AI Agent 说明本仓库的定位与协作规则。进入 `dioxus/` 或 `deepseek-harness/` 时，还需阅读它们各自的 `AGENTS.md`。

## 1. 项目定位

本项目用 Dioxus（Rust）实现 / 翻译 deepseek-harness 的 Web UI，GUI 代码位于 `dsh-dioxus-ui/`。

- 目标是前端层的等价重写：交互流程、界面结构、状态与数据流对齐 deepseek-harness 的现有 Web UI。
- 后端能力（会话、模型调用、工具执行、权限等）以 deepseek-harness 现有实现与协议为准，本项目的 Dioxus 前端作为其客户端消费。
- 判定「翻译对了」的标准是行为一致（同样的输入得到同样的界面与交互结果），不是代码逐行对应。

## 2. 目录约定

```
.
├── AGENTS.md            # 本文件
├── dsh-dioxus-ui/       # 要构建的 dsh GUI 代码（本项目自身代码，Dioxus 实现）
├── dioxus/              # dioxus 原始仓库的 fork clone（origin: git@github.com:ray2kwok/dioxus.git, 分支 main）
└── deepseek-harness/    # deepseek-harness 原始仓库的 fork clone（origin: git@github.com:ray2kwok/deepseek-harness.git, 分支 master）
```

`dsh-dioxus-ui/` 是要构建的 dsh GUI 代码所在目录，本项目的开发工作集中在这里；两份 fork clone 只作参考与构建依赖。

`dioxus/` 与 `deepseek-harness/` 是**原始项目的 fork clone**，用途有两条：

1. **参考**：读代码、确认行为、跑起原实现做对照，是最权威的事实来源。遇到设计疑问先看它们，不要凭猜测实现。
2. **可修改、可构建**：确有必要时（如补齐前端需要的接口、调整打包产物、打本地 patch）可以直接修改并重新构建，不必绕开。

约束：

- 修改子仓库时保持改动最小、可回滚，并在提交信息里说明「为什么必须改上游代码」，以便后续与 upstream 同步。
- 不要把本项目的业务代码放进子仓库；不要把子仓库改成与本项目强耦合的形态。
- 子仓库是 git 仓库，不要在其中执行 `git push --force`、`git reset --hard` 等破坏性操作，除非用户明确要求。

## 3. 参考坐标（deepseek-harness）

| 位置 | 内容 |
| --- | --- |
| `apps/web` | Web 前端入口（Vite + `@deepseek-ai/dsh-client-web` shell 库），产物 `dist/` 由 `apps/cli` 的 `dsh web` 提供 |
| `packages/client/web` | 前端 shell 库本体 |
| `packages/client/ui-*` | 各功能域 UI 子包（chat、sidebar、settings、session、tool、plan、permission 等），单个功能的行为对照优先看这里 |
| `packages/client/{store,connection,locale,shortcuts,resources}` | 状态、连接、i18n、快捷键等前端基础设施 |
| `packages/api`、`packages/sdk`、`packages/core` | 前后端协议与核心能力 |
| `apps/cli`、`apps/desktop` | 宿主进程与打包 |

构建/运行参考（在 `deepseek-harness/` 下）：Node >= 24.9、pnpm 11.28.5；`pnpm --filter @deepseek-ai/dsh-web-frontend run dev` 起前端，`run build` 出产物。

## 4. 参考坐标（dioxus）

本项目的技术栈来源：`dioxus/packages/{core,web,desktop,router,fullstack,signals,stores,html,rsx}`。需要框架用法先查这些 crate 的源码与仓库内 `examples/`，不要凭记忆写 API。

## 5. 工作方式

- 先读后写：改动前先在 `deepseek-harness` 中定位对应 UI/行为，再在 Dioxus 中实现。
- 一次只翻译一个可验证的界面单元，附带如何手工验证的说明。
- 不确定协议字段或行为时，以 `deepseek-harness` 的 TypeScript 类型与实现为准。
