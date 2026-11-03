# RENDER：渲染后端现状与 native 化评估

| 项 | 内容 |
| --- | --- |
| 文档 | 渲染后端评估 + native 方案设计 |
| 项目 | `dsh-dioxus-ui` |
| 版本 | v0.1（评估稿，未落地） |
| 日期 | 2026-11-03 |
| 关联 | `ARCH.md` 的 D1（应用形态）**保持有效**，本文档是其技术复核，未推翻 |
| 上游基线 | `dioxus` @ `0.8.0-alpha.3`、`dioxus-native` @ `0.8.0-alpha.3`、`blitz` @ `0.3.0-beta.4` |

---

## 0. 结论（TL;DR）

1. **现在的渲染确实是 WebView**：`dioxus` 的 `desktop` feature → `dioxus-desktop` → `wry 0.57.1` → 系统 WebView
   （Linux WebKitGTK 2.x / Windows WebView2 / macOS WKWebView）。`Cargo.lock` 里同时锁着
   `webkit2gtk 2.0.2`、`tao 0.37.2`。
   > 注意区分：`crates/render`（`dsh-render`）是 **内容渲染**（Markdown / 高亮 / diff 的纯函数），
   > 与渲染后端无关。本文说的是「谁把 rsx 画到屏幕上」。
2. **native 技术可行**：`dioxus 0.8.0-alpha.3` 提供 `native` feature，绑定
   `dioxus-native 0.8.0-alpha.3`（2026-10-16 发布），底层 `blitz 0.3.0-beta.4` + `winit 0.31.0-beta.3`，
   默认渲染后端 `vello-hybrid`(wgpu)。**版本链与本项目完全对齐**，不需要降级框架。
3. **但当前不满足「行为一致」**：本项目验收口径是行为等价（见 `AGENTS.md`）。native 路径上
   有 **4 个必须先验证的 P0 未知项**：`document::Style` 是否生效、CSS 变量/属性选择器、
   中文 IME、`<textarea>` 受控输入；另有确定的改造面：`overflow: auto`（blitz 不支持）、
   9 处 `document::eval`、文件选择与剪贴板要换实现（P1）。
4. **建议**：不直接切主线。先花 **3–5 人日** 做 §8 的 10 项 spike；全绿才投入 S1–S4
   （约 **25–39 人日**）。**任一 P0 失败或 ≥3 项失败 → 放弃 native 主线**，把精力放回 WebView 打包问题。
5. **无后悔动作**：S1 抽象层（去 JS 依赖、图标改 rsx、滚动改 `MountedData`）**对 WebView 同样有收益**，
   可以先做，与是否上 native 无关。

---

## 1. 现状：渲染链路

```
dsh-app (bin)
  └─ dioxus::LaunchBuilder  (features = ["desktop"])
       └─ dioxus-desktop 0.8.0-alpha.3
            ├─ tao 0.37.2       窗口/事件循环
            └─ wry 0.57.1
                 └─ 系统 WebView：webkit2gtk 2.0.2（Linux）/ WebView2 / WKWebView
                      └─ DOM + CSS 引擎（浏览器内核）
```

**证据**

| 事实 | 位置 |
| --- | --- |
| facade 只开 `desktop`，未开 `native` | `Cargo.toml:39` |
| 桌面入口用 `dioxus::desktop::{WindowBuilder, Config}` | `crates/app/src/main.rs:62-85` |
| `dioxus-desktop 0.8.0-alpha.3` | `Cargo.lock:847` |
| `webkit2gtk 2.0.2` / `wry 0.57.1` / `tao 0.37.2` | `Cargo.lock:5116 / 5546 / 4381` |
| 唯一 CSS 注入点是 `document::Style`（2886 行内联） | `crates/app/src/main.rs:1468` |
| 首帧防闪白靠 `with_custom_head` 里的 JS 脚本 + `localStorage` | `crates/app/src/main.rs:70-83` |

**WebView 形态的收益**：完整 DOM/CSS（509 处 `var(--dsw-*)`、`@media`、伪元素、`backdrop-filter` 全部可用）、
IME 与文本编辑由浏览器内核兜底、产物仅 **4.7 MB**、不打包浏览器内核。

**WebView 形态的代价**：

- Linux 依赖系统 `webkit2gtk-4.1` + `libsoup3`（版本过低/缺失直接起不来，见 `ARCH.md` §20 风险表）；
- 三平台内核差异 → 渲染细节不可控；
- 浏览器「额外能力」只能通过 JS 调用（当前 9 处 `document::eval`）。

---

## 2. 「native」在本项目里指什么

| 候选 | 含义 | 判定 |
| --- | --- | --- |
| **A. `dioxus-native`（blitz）** | 同一份 rsx，换「自绘 HTML/CSS 引擎」：Stylo 解析 CSS + Taffy 布局 + Parley 文本 + vello/wgpu 绘制 | **本文评估对象**，唯一「同代码换后端」的路径 |
| B. WebView 但自带内核 | 例如打包 WebKitGTK / CEF / Servo shell | 体积 +100 MB 量级，且 Servo 嵌入更不成熟，**放弃** |
| C. 换 UI 框架（egui / iced / slint） | 立即模式或自有 DSL 重写 | 17 个 `#[component]`、4393 行 rsx 全废，**放弃** |

结论：只有 A 具备讨论价值。它保留 rsx / signal / router 全部上层代码，替换的是「谁执行布局与绘制」。

---

## 3. 版本与构建链路核对（已对源码/发布物确认）

| 事实 | 依据 |
| --- | --- |
| `dioxus` 的 `native` feature = `dep:dioxus-native` + `dioxus-config-macro/native` | `dioxus-0.8.0-alpha.3/Cargo.toml:128-130` |
| `native` 与 `desktop` 可同时存在，但 **native 优先**（`LaunchBuilder` 按 native→desktop→mobile 顺序判定） | `dioxus-0.8.0-alpha.3/src/launch.rs:21-33、102、335-338` |
| `dioxus-native 0.8.0-alpha.3` 精确锁 `dioxus-core =0.8.0-alpha.3`、`blitz-* =0.3.0-beta.4`、`winit =0.31.0-beta.3` | docs.rs 依赖表 |
| `dioxus-native` 默认 feature 含 `vello-hybrid`(wgpu)、`accessibility`、`clipboard`、`file-dialog`、`net`、`svg`、`woff`、`system-fonts`；`scrollbars`、`data-uri`、`skia`、`vello-cpu` **非默认** | docs.rs feature 列表 |
| `dx 0.8.0-alpha.3` 的 `--platform` 枚举**没有** `native`，但 `--renderer` 有 `native` | `dioxus-cli-0.8.0-alpha.3/src/platform.rs:12-56、153-171、193-206` |

构建方式（二选一，前者更省事；`renderer-native` feature 按 §7.6 新增后可用）：

```bash
# 方式 1：直接 cargo（不经过 dx 的 wasm/asset 流程）
cargo build -p dsh-app --release --no-default-features --features renderer-native

# 方式 2：走 CLI（renderer 是独立开关）
dx build --platform linux --renderer native --release
```

> 风险：整条链是 `=` 精确锁定且全是 pre-release。**任何** `cargo update` 都可能把
> `dioxus-core` 抬到 alpha.4 而 `blitz` 仍锁 beta.4，直接解析失败。native 分支应单独
> 维护 `Cargo.lock` 或在 workspace 里固定 patch 版本。

---

## 4. 能力核对（按项目实际用到的逐项过）

### 4.1 后端无关部分：零改动

| crate | 行数 | 与渲染后端的关系 |
| --- | --- | --- |
| `dsh-proto` | 2450 | 无（纯 serde 类型） |
| `dsh-transport` | 1004 | 无（reqwest + tungstenite + 子进程） |
| `dsh-state` | 599 | 无 |
| `dsh-runtime` | 970 | 无 |
| `dsh-render` | 887 | 无（Markdown/高亮/diff 纯函数） |
| 合计 | **5910** | **一行不改** |

需要动的是 `dsh-ui`(4393) 与 `dsh-app`(3019) 里与平台耦合的少数点，以及 2886 行 CSS 中的个别属性。

### 4.2 布局与 CSS

| 特性 | 项目用量 | blitz 状态 | 处置 |
| --- | --- | --- | --- |
| `display: flex` | 62 处 | ✅ 完全支持 | 不动 |
| `display: grid` | 少量 | ✅（无 subgrid） | 不动 |
| `position: absolute/fixed` | 3 处 fixed | ✅ | 不动 |
| `position: sticky` | **0 处（未用）** | 🚧 进行中 | 不受影响 |
| `border-radius` / `overflow: hidden` / `box-shadow` | 53 / 若干 / 9 | ✅ | 不动 |
| `transition` / `@keyframes` | 各 1 处 | ✅ | 不动 |
| **`overflow: auto`** | **11 处**（`main.css:133,240,379,816,860,1106,1237,1376,1425,1828,1925`） | ❌（仅 `visible/clip/hidden/scroll`） | 改 `overflow: scroll`；或在 `native.css` 覆盖 |
| **CSS 变量 `var(--dsw-*)`** | **509 处** | ⚠️ 待验（stylo 支持自定义属性，但 blitz 的层叠/继承顺序需实测） | spike #2；主题体系全靠它，**这是最高杠杆的验证项** |
| **属性选择器 `body[data-ds-dark-theme]`** | 9 处（`main.css:5`、`design-platform.css` 3、`scrollbar.css` 1；全部 `[data-*]` 形态共 13 处） | ⚠️ 待验 | 同上 |
| `@media (max-width: …)` | 3 处（`main.css:2073,2082`；`scrollbar.css:87`） | ⚠️ 待验 | 失败只是失去响应式布局，可接受 |
| `backdrop-filter: blur` | 1 处（`main.css:881`） | ⚠️ 仅 Skia 后端 | 改纯色半透明；或 native 构建开 `skia` |
| **`::-webkit-scrollbar*`** | `scrollbar.css:67` + `@supports not selector()` 门控（:45） | ❌ | `scrollbar-width` / `scrollbar-color` 已支持；需开 `scrollbars` feature |
| `text-overflow` / `white-space` | 34 处 | ⚠️ 待验 | 失败只是省略号缺失 |
| `text-shadow` / `mix-blend-mode` | 0 处 | ❌（不用） | 不受影响 |
| SVG 的 `fill`/`stroke` **CSS** | 0 处（图标走 `currentColor` 属性） | ❌ CSS 形式不支持 | 见 §4.5 |

### 4.3 文本与输入（P0，全部需实测）

| 能力 | 现状用法 | native 判定 |
| --- | --- | --- |
| `<textarea>` 受控输入 | `crates/ui/src/composer.rs:168`（唯一主输入框，`rows/placeholder/disabled/value/oninput`） | ⚠️ 待验。blitz 已有 `events/ime.rs`，但 dioxus-native 的 `convert_composition_data` 曾长期是 `unimplemented!()` 占位；该错误记录在 2026-09 后已从静态索引撤下，**不能据此判断已修复** |
| 中文 IME 组合（拼音候选框、上屏） | 同上 | ⚠️ **最高优先级 P0**：失败则本项目不可用（界面与输入全中文） |
| `Enter` / `Shift+Enter` / `Escape` / 方向键 | `composer.rs:190-209`、`conversation.rs:167`、`sidebar.rs:109,203` | ⚠️ 待验（`evt.key()` 映射、`prevent_default` 是否生效） |
| 文本选择 / Ctrl+C / 右键菜单 | 全局 | ⚠️ 待验（blitz `user-select` 仅部分生效） |
| 焦点管理（`autofocus`） | 少量 | feature `autofocus` 非默认，需显式开 |
| CJK 字形 | 系统字体 | ⚠️ 需开 `system-fonts`（默认开）+ Linux 侧 fontconfig 有中文字体；缺字体 = 方块 |
| `<input type=file>` + `evt.files()` + `read_bytes()` | `composer.rs:264-282` | ❌ 大概率不支持 → 走 `dioxus-native` 的 `file-dialog`（默认开启，系统对话框），需抽象成 `pick_files()` |
| 拖拽上传 | 未实现 | 不受影响 |
| `<select>` | 0 处（全自定义 div） | 不受影响 |

### 4.4 无 JS：9 处 `document::eval` 的替代

native 没有 JS 引擎（`dioxus-document` 在无 JS 平台上是 `NoOpDocument`，`eval` 只会返回
`EvalError::Unsupported` 或无效果），因此这 9 处必须重写：

| # | 位置 | 用途 | 替代方案 |
| --- | --- | --- | --- |
| 1 | `app/src/main.rs:835` | 复制消息（`navigator.clipboard`） | `dsh-platform::write_clipboard()`；native 用 `clipboard` feature（默认开启），必要时直接接 `arboard` |
| 2 | `app/src/main.rs:2174` | `apply_appearance`：写 `body[data-ds-dark-theme]`、`color-scheme`、CSS 变量、`localStorage` | **改成纯 rsx**：根容器上挂 `data-ds-dark-theme` 属性 + `style` 里写自定义属性；主题持久化已有 Host `ui-theme` 设置，本地缓存改用配置文件 |
| 3-6 | `app/src/main.rs:1120,1125,1131,1137` | 设置页改字号/字体时的即时预览（写 `--dsh-*-font-size` 等） | 同 #2：把变量写成根容器的 inline `style`，随 signal 重渲，**彻底去掉 eval** |
| 7 | `ui/src/conversation.rs:106` | 新消息时滚到底（`querySelector('.messages')`） | `onmounted` 存 `MountedData` → `get_scroll_size()` + `scroll(0, height)`；`dioxus-desktop` 已实现该 API，native 待验 |
| 8 | `ui/src/conversation.rs:301` | 轨迹行点击滚到 `[data-seq]` | 行级 `onmounted` 收集 `MountedData`（只收集可见行，成本可控）→ `scroll_to()` |
| 9 | `ui/src/file_tree.rs:114` | 复制路径 | 同 #1 |

> **收益不只属于 native**：#1/#7/#8/#9 去掉的是对 DOM 结构的硬编码依赖（`querySelector`），
> 改完 WebView 版同样更稳；#2–#6 去掉的是「用 JS 改 CSS」，改完主题/字号是纯数据流。

### 4.5 图标与资源

| 项 | 现状 | native 风险 | 处置 |
| --- | --- | --- | --- |
| 图标 | `ui/src/icons.rs:23` 用 `dangerous_inner_html` 塞 14 条 SVG path | ⚠️ `SetInnerHtml` 依赖 HTML 解析，native 未确认支持 | **改写成 rsx `<path>`**（14 个，工作量小，两端受益，顺带消除注入面） |
| 图片（会话内） | `img src`，`/api/file?path=` 与 `data:` base64 | `blitz-net` 默认开；`data:` 需 `data-uri` feature | 开 feature 即可 |
| 字体 | 系统字体栈 | `system-fonts` 默认开；WOFF 需 `woff`（默认开） | 不动 |
| CSS 打包 | `concat!(include_str!(...))` + `document::Style` | `document::Style` 在 native 会把 `<style>` 插进 blitz 的 `<head>`（源码已确认 `create_head_element` 实现），但是否触发布局重算未验 | spike #1 |
| `asset!` / manganis | **未使用**（CSS 走 `include_str!`） | 无 | 不受影响 |

### 4.6 窗口 / 启动 / 路由

| 项 | 现状 | native |
| --- | --- | --- |
| 窗口 | `desktop::WindowBuilder::new().with_title(...).with_inner_size(1280×860).with_min_inner_size(960×600)` | `native::Config::new().with_window_attributes(WindowAttributes)`；`WindowAttributes` 即 winit 0.31 的类型，方法名 spike 时确认 |
| 菜单 | `with_menu(None)` | native 无菜单，天然一致 |
| 自定义 head（防闪白脚本） | `with_custom_head` | native 无此 API → 用 `Config::with_base_color(...)` 直接设底色 |
| 路由 | `MemoryHistory` | 与渲染无关，不受影响 |
| 多窗口 / 托盘 | 未使用 | 不受影响 |
| `use_window` 等 desktop 专有 API | **仅 `main.rs` 用到** | 需要 cfg 分支（见 §7.5） |

### 4.7 体积 / 内存 / 性能（预期，均需实测）

| 维度 | WebView（现状） | native（预期） |
| --- | --- | --- |
| 产物 | 4.7 MB（不打包内核） | **预计 25–45 MB**（wgpu + vello + stylo + parley + taffy + winit + 字体栈），显著变大 |
| 常驻内存 | WebKit 渲染进程（不可控） | 无子进程，但 wgpu 显存常驻 |
| 启动 | WebView 初始化 + JS 引擎 | 无 WebView 初始化；需初始化字体库与 GPU 上下文 |
| 增量刷新 | 浏览器内核优化完备 | blitz 以整帧重绘为主，长会话滚动需实测 |
| 系统依赖 | `webkit2gtk-4.1` + `libsoup3` | 无（只有 GPU 驱动） |

> **口径澄清**：native 不以「更小更省」为目标。它买到的是「**不依赖系统 WebView**」
> （部署、内核一致性），代价是体积、CSS 子集、重新实现平台能力。若动机是「省体积」，
> native 是反向的，本方案不适用。

---

## 5. 风险矩阵

| 级别 | 风险 | 判定方法 | 失败兜底 |
| --- | --- | --- | --- |
| **P0** | 中文 IME 无法上屏 | spike #4：输入「你好」观察候选与上屏 | 放弃 native（无替代方案） |
| **P0** | `<textarea>` 受控输入行为异常（光标跳动/丢字符） | spike #4：连续输入 + 删改 + Shift+Enter | 放弃 native |
| **P0** | CSS 变量不生效 → 整套主题失效 | spike #2：509 处变量抽 3 个抽样验证 | 放弃 native（改造成本不可控） |
| **P0** | `document::Style` 不生效 → 全页无样式 | spike #1：背景色置红 | 改用 blitz `DocumentConfig` 注入 |
| P1 | `overflow: auto` 不支持 | 静态已知 | 统一改 `scroll` / native.css 覆盖 |
| P1 | 文件选择不可用 | spike #5 | 用 `file-dialog` feature（必要时直接接 `rfd`）实现 `pick_files()` |
| P1 | 剪贴板不可用 | spike #6 | `arboard` 直连 |
| P1 | 滚动 API（`MountedData`）未实现 | spike #7 | 自管偏移 + `onscroll` |
| P1 | `dangerous_inner_html` 不支持 | spike #8 | 图标改 rsx（已定） |
| P1 | 中文字体缺失 | spike #10 | 内嵌一份 WOFF 子集（+2–4 MB） |
| P2 | `@media` 不生效 | 静态已知 | 失去响应式，固定布局 |
| P2 | 滚动条样式 | 静态已知 | 开 `scrollbars` feature，接受 UA 样式 |
| P2 | `backdrop-filter` 无效 | 静态已知 | 半透明纯色 |
| P2 | 长列表滚动掉帧 | spike #9 | 真虚拟列表（本就在 M5 待办） |

---

## 6. 路线对比

| | A. 维持 WebView | B. 全量切 native | C. 双后端可插拔（推荐） |
| --- | --- | --- | --- |
| 功能风险 | 0 | 高（4 个 P0） | 低（默认路径不变） |
| 交付节奏 | 不变 | 主线停摆 1–2 月 | 主线不停，native 独立推进 |
| 系统依赖 | webkit2gtk / WebView2 / WKWebView | 仅 GPU | 取决于构建 |
| 体积 | 4.7 MB | 25–45 MB | 两条产物各自最优 |
| 代码复杂度 | 低 | 中（9 处 eval + 平台层） | 中（同左，但可控） |
| 放弃成本 | — | 高（改完才发现 IME 不通） | 低（spike 失败即止损） |

**推荐 C**。核心理由：C 的前半段（去 JS 依赖）无论如何都该做，且**不引入任何功能风险**；
后半段（native 后端）用一次 spike 决定，失败时损失上限 = 5 人日。

---

## 7. 方案设计（C：渲染后端可插拔）

### 7.1 crate 结构

```
crates/
├── platform/           # 新增 dsh-platform：唯一与「渲染后端」耦合的地方
│   ├── src/lib.rs      # 能力函数（cfg 选择实现），不含 UI
│   ├── src/webview.rs  # eval / 系统 WebView 路径
│   └── src/native.rs   # arboard / rfd / 配置文件路径
├── ui/                 # 只依赖 dsh-platform 的函数，不感知后端
└── app/
    ├── src/main.rs     # 仅剩 fn main()：解析 env → 选后端 → launch
    ├── src/app.rs      # 现有 App 组件（原 main.rs 主体，与后端无关）
    ├── src/launch_desktop.rs
    └── src/launch_native.rs
```

依赖方向：`app → ui → platform`（`platform` 不依赖 `ui`），`proto/transport/state/runtime/render` 完全不动。

### 7.2 `dsh-platform` 的能力清单（刻意不做 trait）

单一构建只有一个后端，用 trait 抽象是多余间接层；直接 `#[cfg]` 选函数体即可。

```rust
// 全部为同步/异步自由函数，签名与后端无关
pub fn write_clipboard(text: &str);
pub fn load_pref(key: &str) -> Option<String>;      // 窗口尺寸、主题缓存
pub fn save_pref(key: &str, value: &str);
pub async fn pick_files(multiple: bool) -> Vec<PickedFile>;   // name / media_type / bytes
pub fn base_color() -> (u8, u8, u8);                // native 清屏色 / webview 无操作
pub const BACKEND: Backend;                         // Webview | Native，用于诊断面板展示
```

**不在抽象范围内的**（已确认可做到后端无关）：

- 滚动：统一用 `MountedData::scroll / scroll_to / get_scroll_size`（`dioxus-desktop` 已实现，native 待验）；
- 主题与 CSS 变量：统一用 rsx 的属性/inline `style`（见 §7.4）；
- 图标：统一用 rsx `<path>`。

### 7.3 九处 eval 的收敛顺序（可独立提交）

| 提交 | 内容 | 影响面 | 可回归验证 |
| --- | --- | --- | --- |
| P-1 | #2–#6 主题/字号改 rsx 属性 + inline style；CSS 里 `body[data-ds-dark-theme]` → `[data-ds-dark-theme]`（全部 9 处） | 主题、设置页 | 切主题、改字号、冷启动不闪白 |
| P-2 | #7/#8 滚动改 `MountedData` | 会话视图、轨迹 | 新消息自动到底；点轨迹行跳转 |
| P-3 | #1/#9 剪贴板走 `dsh-platform` | 消息复制、文件名复制 | 复制后粘贴到外部编辑器 |
| P-4 | 图标改 rsx `<path>` | 全部图标 | 图标颜色随主题变化 |
| P-5 | 文件选择走 `pick_files()` | 附件 | 选图/选文件上传成功 |

P-1～P-5 全部在 WebView 上可验证、可回归，**与 native 无关**，建议作为独立里程碑先做（约 4–6 人日）。

### 7.4 样式分层

```
assets/
├── theme/{base,design-platform,scrollbar,focus,corner-shape}.css   # 共享（现状）
├── main.css                                                        # 共享主体
└── native.css                                                      # 仅 native：追加覆盖，不修改上游 token
```

`native.css` 只做四件事：

```css
/* 1. blitz 不支持 overflow: auto：把 main.css 里 11 处滚动容器逐个改 scroll */
/*    （或在此对同一批选择器覆盖 overflow；不要用 * 兜底，会破坏 fixed 布局） */

/* 2. 滚动条：标准属性取代 ::-webkit-scrollbar */
* { scrollbar-width: thin; scrollbar-color: var(--dsh-scrollbar-thumb) transparent; }

/* 3. backdrop-filter 降级 */
/*    针对 main.css:881 那条规则的选择器覆盖为 none + 半透明底色 */

/* 4. 主题宿主从 body 移到根容器（若 blitz 对 body 属性选择器有偏差） */
/*    给 AppShell 最外层加统一类名（如 .app），把 design-platform 里
      body[data-ds-dark-theme] 的 token 覆盖规则复制一份到 .app[data-ds-dark-theme] */
```

加载顺序：`theme/* → main.css → native.css`，native 覆盖永远在最后，WebView 构建不包含该文件。

### 7.5 入口拆分

```rust
// main.rs
fn main() {
    init_tracing();
    #[cfg(feature = "renderer-native")]
    launch_native::run();
    #[cfg(not(feature = "renderer-native"))]
    launch_desktop::run();
}
```

```rust
// launch_desktop.rs（现状搬迁 + 保留 custom_head 防闪白）
let cfg = dioxus::desktop::Config::new()
    .with_window(WindowBuilder::new().with_title("dsh")
        .with_inner_size(LogicalSize::new(1280.0, 860.0))
        .with_min_inner_size(LogicalSize::new(960.0, 600.0)))
    .with_menu(None)
    .with_custom_head(BOOT_SCRIPT);
dioxus::LaunchBuilder::new().with_cfg(cfg).launch(App);

// launch_native.rs
let cfg = dioxus::native::Config::new()
    .with_window_attributes(
        WindowAttributes::default()
            .with_title("dsh")
            .with_inner_size(LogicalSize::new(1280.0, 860.0)),
    )
    .with_base_color(base_color());
dioxus::LaunchBuilder::new().with_cfg(cfg).launch(App);
```

### 7.6 feature 矩阵

| feature | 依赖 | 用途 |
| --- | --- | --- |
| `renderer-webview`（默认） | `dioxus/desktop` | 现行交付 |
| `renderer-native` | `dioxus/native` + `dioxus-native/{scrollbars,data-uri,autofocus}` | 实验后端 |

`dsh-app` 只保留两个互斥 feature，**禁止同时开启**（`launch` 会静默选 native，容易误判构建产物）。

### 7.7 对 WebView 主线的保证

1. 默认 feature 不变 → 产物、体积、启动链路零变化；
2. CI 增加一条 `cargo check --no-default-features --features renderer-native`（只编译，不跑）；
3. `doc/ARCH.md` 的 D1 不改写；本文档只作为「D1 的复核与备选路径记录」。

---

## 8. Spike 计划（3–5 人日，决定 go/no-go）

新建临时 crate `crates/spike-native`（只依赖 `dioxus = { features = ["native"] }`），
一屏铺 10 个探针，每个探针「通过 = 预期现象出现」。

| # | 探针 | 通过标准 | 失败后果 |
| --- | --- | --- | --- |
| 1 | `document::Style` 内联 200 行 CSS | 背景/边框/圆角按预期渲染 | 改 `DocumentConfig` 注入，或放弃 |
| 2 | CSS 变量 + 属性选择器主题（3 个 token + 5 条 `[data-*]` 规则） | 切换属性后颜色即时变化 | **放弃** |
| 3 | flex/grid/absolute/fixed + `border-radius` 裁剪 | 与 WebView 截图肉眼一致 | 样式返工 |
| 4 | `<textarea>` 中文 IME + 受控 `value` + Enter/Shift+Enter | 「你好世界」可上屏、光标不跳、换行正确 | **放弃** |
| 5 | `input type=file` / `rfd` 文件选择并读取字节 | 选中文件拿到 `Vec<u8>` | 用 `rfd` 自实现 |
| 6 | 剪贴板写入 + 系统粘贴 | 外部编辑器能粘贴 | `arboard` 直连 |
| 7 | `MountedData::scroll / scroll_to / get_scroll_size` | 300 条消息可滚到底、可定位某条 | 自管滚动状态 |
| 8 | `dangerous_inner_html`（SVG 片段） | 图标出现 | 改 rsx `<path>`（已定） |
| 9 | 3000 行 Markdown 列表滚动 | 无明显卡顿（≥30 fps） | 真虚拟列表 |
| 10 | 中文渲染 + 粗体/斜体/代码字体 | 无方块、字重正确 | 内嵌字体子集 |

**判定**：10/10 → 进 S2；8–9 → 先修 P1 再评估；≤7 或有任一 P0 失败 → 停止 native 路线，
把结论回写本文档「附录 A」，并转向 WebView 打包问题（固定基础镜像 / 提供 AppImage）。

---

## 9. 里程碑与工作量

| 阶段 | 内容 | 人日 | 交付判定 |
| --- | --- | --- | --- |
| S0 | §8 spike | 3–5 | go/no-go 结论 |
| S1 | `dsh-platform` + 9 处 eval 收敛 + 图标 rsx + 入口拆分（**WebView 可验证**） | 4–6 | WebView 行为零回归 |
| S2 | native 首屏：拉 Host → 列表 → 选会话 → 快照渲染 | 5–8 | 能看到真实会话 |
| S3 | native 交互对齐：输入/流式/滚动/审批/设置/文件树/子 Agent | 8–12 | S1–S9 手工清单过 |
| S4 | 样式收敛 + 三平台验证 + 体积/内存实测 | 5–8 | 与 WebView 截图逐屏对比 |
| 合计 | | **25–39** | S1 独立价值已兑现，S2 之后为增量 |

> S1 的 4–6 人日**不依赖** spike 结论即可开工（它修的是现有 WebView 实现的技术债）。

---

## 10. 验收清单（native 专项，S3 之后）

沿用 `PR.md` 的 S1–S9，另加：

- [ ] 中文输入：拼音候选框位置正确，整句上屏，退格不吞字
- [ ] 长会话（≥500 条）滚动到底、向上翻页、轨迹跳转
- [ ] 流式输出期间滚动不抖、不跳回顶部
- [ ] 复制消息 / 复制路径 → 系统剪贴板可用
- [ ] 附件：图片预览、文件上传、失败提示
- [ ] 主题切换（深/浅/跟随系统）+ 字号 + 字体族即时生效且持久化
- [ ] 窗口缩放至 960×600 布局不破（`@media` 若失效需回归到固定布局）
- [ ] 断网重连：杀 Host → 500ms 退避 → 新 Host → generation+1
- [ ] 冷启动无白屏闪烁
- [ ] 无 WebKitGTK 的干净 Linux 镜像可直接运行

---

## 11. 决策建议

1. **现在**：不动主线渲染。当前 WebView 形态满足「行为一致」这一唯一验收口径。
2. **近期（4–6 人日，无风险）**：做 §7.3 的 P-1～P-5。改完 9 处 `document::eval` 只剩 2 处
   （两处剪贴板，且都被收进 `dsh-platform::webview` 内部），顺带解决 `querySelector` 硬编码 DOM 结构的技术债。
3. **并行（3–5 人日）**：跑 §8 spike。这是唯一能回答「native 可行吗」的方式——
   文档只能给出证据与边界，IME / 滚动 / 变量这三项必须用真机代码验证。
4. **判定后**：
   - 全绿 → 按 S2–S4 推进，native 作为**可选产物**与 WebView 并行发布；
   - 有 P0 失败 → 关闭 native 路线，把同样的精力投到「Linux 打包（AppImage + 固定 WebKitGTK）」
     与「三平台渲染差异收敛」，收益更确定。

---

## 12. 目标设备复核：RK3588（aarch64 + Mali-G610）

> **结论：不必要。** 除非命中 §12.3 的三条判据之一。本节只针对「3588 接屏、本机跑 GUI」，
> 3588 仅作无头 Host 时连本项目 GUI 都不需要跑（浏览器远程访问即可）。

### 12.1 为什么不必要

1. **失败模式更硬**：WebKitGTK 在 Mali 上最坏是退回软渲染——慢，但仍可用（可用环境变量判定是合成器
   还是驱动问题）；native 走 wgpu，Vulkan/GLES 初始化失败就是黑屏，没有等价降级路径，
   而它想解决的恰恰是「慢」。
2. **UI 不是该设备的瓶颈**：本架构是「Node Host + 瘦客户端」，3588 上 CPU 大头是 Host
   （模型调用、会话存储、工具执行、文件扫描）。把 UI 渲染换成自绘，省下的那部分会被
   blitz 的整帧重绘（无浏览器级局部合成优化）抵掉大半。
3. **功能面没有改善**：IME / CSS 子集 / 文件选择这些缺口在 aarch64 上一样存在；
   winit 侧的输入法（X11 XIM / Wayland text-input-v3）通常**弱于** WebKitGTK，
   而本项目界面与输入全中文。
4. **依赖面没省多少**：Debian/Ubuntu arm64 有官方 `libwebkit2gtk-4.1` 包，装机成本低；
   native 反而要求 Mesa/GPU 驱动栈完整（PanVK 或厂商 blob），RK3588 上这本身是个老问题。

### 12.2 先做的便宜事（0.5–2 人日，按顺序）

| # | 动作 | 判定 / 说明 |
| --- | --- | --- |
| 1 | 量基线：启动耗时、长会话滚动帧率、常驻 RSS | 没有基线就没有「要不要切」的依据 |
| 2 | `WEBKIT_DISABLE_DMABUF_RENDERER=1` | RK3588 上最常见的修复；若帧率明显回升，说明是 DMABUF 合成问题 |
| 3 | `WEBKIT_DISABLE_COMPOSITING_MODE=1` | **强制软渲染**，用来区分「合成器/GPU 问题」还是「布局本身太重」 |
| 4 | `GDK_BACKEND=wayland`（若现在是 X11） | RK3588 上 X11 的 GL 路径普遍更差，Wayland 通常明显更好 |
| 5 | 中文环境：fcitx5 + Noto CJK + `fc-list :lang=zh` 自检 | 缺字体/输入法会在任何后端上表现为方块或无候选 |
| 6 | 关掉 `main.css:881` 的 `backdrop-filter` 与唯一的 `@keyframes` | 全项目各 1 处，成本几乎为零 |
| 7 | 消息窗口化（M5 待办，最近 300 条） | 与 §16 的长期计划重合，做了对两端都有益 |

### 12.3 只有这三种情况才该考虑 native

| 判据 | 怎么验 | 说明 |
| --- | --- | --- |
| **发行版没有可用的 `libwebkit2gtk-4.1`**（Yocto/Buildroot 定制镜像、版本过低） | `pkg-config --modversion webkit2gtk-4.1` | 这是最硬的理由：native 的运行时依赖只剩 GPU 驱动 |
| 只能软渲染且帧率不可接受，同时 GPU 可用 | 滚动帧率达不到可接受阈值（按体验定，建议先以 30 fps 为目标）；且板子上 `vulkaninfo` 能过 | 两个条件同时成立才成立，缺一不可 |
| 业务要求跨设备像素一致（演示/验收） | 业务判定 | WebView 三平台内核差异是本项目已知风险 |

### 12.4 若确实要做，额外注意

- 后端选择：`vello`（Vulkan）优先；`vello-cpu` 只作兜底——**兜底即软渲染，等于没有性能收益**，
  只能解决「WebKitGTK 装不上」这一类问题。
- 交叉编译 `aarch64-unknown-linux-gnu`，系统依赖：fontconfig、wayland/x11 开发库。
- `system-fonts` 依赖宿主机 fontconfig；板子必须装中文字体，否则方块。
- winit 0.31（beta）在 Wayland 下的 IME 需实测（fcitx5 / text-input-v3），这是 ARM 场景下的新增 P0。
- 体积 25–45 MB：单机部署无所谓；若走 OTA 镜像需计入。

---

## 附录 A 证据与命令

```bash
# 当前依赖是否含 blitz / native（应无输出）
grep -nE 'name = "(dioxus-native|blitz-dom|vello|wgpu)"' Cargo.lock

# 当前 WebView 链路
grep -nE 'name = "(dioxus-desktop|wry|webkit2gtk|tao)"' -A1 Cargo.lock

# 9 处 JS 依赖
grep -rn "document::eval" --include=*.rs crates

# CSS 上 native 的已知缺口
grep -rnE "overflow: *auto|backdrop-filter|position: *sticky|-webkit-scrollbar" crates/app/assets
```

关键结论的外部依据：

- `dioxus-native 0.8.0-alpha.3`：<https://docs.rs/crate/dioxus-native/0.8.0-alpha.3>（依赖 `blitz 0.3.0-beta.4`、`winit 0.31.0-beta.3`，默认 `vello-hybrid`）
- blitz CSS 支持状态：<https://blitz.is/status/css>（`overflow: auto` ❌、`sticky` 🚧、`backdrop-filter` 仅 Skia、SVG `fill/stroke` CSS ❌）
- blitz 项目状态（beta）：<https://github.com/DioxusLabs/blitz>
- `dx` 的 renderer 枚举（含 `native`）：`dioxus-cli-0.8.0-alpha.3/src/platform.rs:153-206`

## 附录 B 术语

| 术语 | 含义 |
| --- | --- |
| WebView 后端 | `dioxus-desktop`：wry 把 rsx 编成 DOM 操作，交给系统浏览器内核渲染 |
| native 后端 | `dioxus-native`：rsx → blitz DOM（Stylo/Taffy/Parley）→ wgpu 绘制，无浏览器内核、无 JS |
| P0/P1/P2 | P0 = 失败即不可用；P1 = 需改造才能等价；P2 = 可接受降级 |
| 行为等价 | 同样输入得到同样的界面与交互结果（`AGENTS.md` 的验收标准） |
