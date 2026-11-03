# RESOURCE：dsh-dioxus-ui 与 Host 的资源占用实测

| 项 | 内容 |
| --- | --- |
| 文档 | 资源占用基线（Resource Baseline） |
| 项目 | `dsh-dioxus-ui` —— Dioxus 实现的 deepseek-harness 本地 GUI |
| 版本 | v1（首次实机采样） |
| 日期 | 2026-11-03 |
| 上游基线 | `deepseek-harness` @ `ce7073d5`（master，depth 1）；`dioxus` @ 0.8.0-alpha.3 |
| 采样形态 | **debug 构建**、GUI 空闲（无 turn 运行、无模型调用） |
| 关联 | 设计取舍见 [`ARCH.md`](ARCH.md) §16；范围见 [`PR.md`](PR.md) |

---

## 1. 采样环境

| 项 | 值 |
| --- | --- |
| 硬件 | ARM Cortex-A55 × 8 @ 1.8 GHz（RK35xx，`aarch64`） |
| 内存 / 磁盘 | 7929 MB / eMMC 57 GB（采样时占用 50%） |
| 系统 | Armbian 26.8.3（Ubuntu 26.04 resolute），kernel `6.1.115-vendor-rk35xx` |
| 会话 | Wayland（`wayland-0`） |
| WebView | WebKitGTK **2.52.6** + libsoup 3.6.6 |
| 工具链 | rustc/cargo **1.99.1**、node **24.20.0**、pnpm **11.28.5** |
| 二进制 | `target/debug/dsh-dioxus-ui`，321 MB（未 strip，含 debuginfo） |

采样时刻：GUI 已连续运行约 7 分钟，窗口在后台，无用户交互。

## 2. 采样方法

```bash
# 进程树与线程数
ps -o pid,ppid,nlwp,%cpu,rss,vsz,etime,comm -p <pids>

# 内存：PSS 才是跨进程可加的口径（WebKit/GTK 的 .so 被多个进程共享）
grep -E '^(Rss|Pss|Shared_|Private_|Anonymous|Swap)' /proc/<pid>/smaps_rollup

# CPU：两次读 /proc/<pid>/stat 的 utime+stime（jiffies，CLK_TCK=100）求差
#   窗口 T 秒内的单核百分比 = Δjiffies / (T × 100) × 100
```

口径说明：

- **PSS** 按共享页的实际分摊计入，各进程相加等于整机真实增量；**RSS** 会把共享库重复计数，只作参考。
- CPU 百分比以**单核**为 100%；8 核机器上「2%」≈ 0.02 核。
- 内存/CPU 是稳态值；启动瞬时会更高，见 §4。

## 3. 进程模型与内存

GUI 不是单进程，实际由 4 个进程组成（GUI 拉起 Host，WebKit 再分裂出渲染与网络进程）：

| 进程 | 角色 | 线程 | PSS | RSS | 峰值 RSS |
| --- | --- | --- | --- | --- | --- |
| `dsh-dioxus-ui` | Rust 侧主进程（`dsh-app`） | 26 | **76 MB** | 152 MB | 152 MB |
| `WebKitWebProcess` | 渲染 rsx / 执行 `document::eval` | 13 | **88 MB** | 155 MB | 156 MB |
| `WebKitNetworkProcess` | WebView 网络（本应用几乎不用） | 12 | 18 MB | 56 MB | 54 MB |
| `node …/bin.ts web` | Host（deepseek-harness） | 11 | **348 MB** | 352 MB | 344 MB |
| **合计** | | **62** | **≈ 519 MB** | ≈ 704 MB | ≈ 700 MB |

- 占整机内存 **约 6.5%**（519 / 7929 MB）。
- 构成：**Host 67% / WebKit 渲染 20% / Rust 主进程 15%**。
- Rust 主进程的 PSS 里有 96 MB 是共享的 GTK/WebKit 映射，**私有脏页只有 27 MB**——即真正由我们代码分配的部分很小。
- Rust 主进程的 26 个线程里，多数是 GTK 主循环、WebKit 辅助线程与 dioxus-desktop 的 tokio 多线程 runtime，不是业务线程。

## 4. CPU

| 进程 | 空闲瞬时 | 自启动平均（7 min） | 累计 CPU |
| --- | --- | --- | --- |
| `dsh-dioxus-ui` | ≈ 2.0% | 0.6% | 2 s |
| `WebKitWebProcess` | ≈ 0% | 0.7% | 3 s |
| `WebKitNetworkProcess` | 0% | 0.0% | 0 s |
| `node` Host | 0% | 2.1% | 9 s |
| **合计** | **≈ 2%（0.02 核）** | ≈ 3.4% | 14 s |

- 空闲基本不吃 CPU；瞬时 2% 主要来自 WebKit 的周期性重绘。
- Host 的 2.1% 是自启动平均值，包含 tsx 编译/模块加载的一次性开销；稳定后降为 0%。
- 已知噪声：日志里的 `Changing the props of Style {}` 每帧会多触发一次文档元素更新，属于 Dioxus 文档元素的重复渲染告警，量级很小，但可以顺手消掉。

## 5. 启动耗时

| 阶段 | 耗时 | 依据 |
| --- | --- | --- |
| Host 冷启动（typert 已生成，跳过生成） | **5.3 s** | `dev-host.sh --no-typert` → 打印 `dsh web:` |
| Typert 描述符生成（默认每次运行都做） | **≈ 20.7 s** | 36 个包 |
| 应用启动 → `/connect` 握手完成 | 34.7 s | 日志：首行 15:39:10.1 → 握手 15:39:44.8 |
| 握手 → 首会话 `follow` 快照就绪 | +0.26 s | 15:39:44.81 → 15:39:45.08 |
| **总计：双击到首屏可用** | **≈ 35 s** | |

结论：**60% 的启动时间花在重复生成 typert 描述符**。`dev-host.sh` 已支持 `--no-typert`，但 `HostConfig.bin` 只能传路径、不能传参数，所以 `DSH_BIN=./scripts/dev-host.sh` 拿不到该开关。加一个带 `--no-typert` 的包装脚本即可把总启动压到约 13 s。

## 6. 磁盘占用

| 项 | 大小 | 说明 |
| --- | --- | --- |
| `dsh-dioxus-ui/crates` | 808 KB | 本项目全部源码 |
| `dsh-dioxus-ui/target` | **3.1 GB** | debug 构建缓存（含依赖） |
| `target/debug/dsh-dioxus-ui` | **321 MB** | 未 strip、带 debuginfo |
| `deepseek-harness` | 2.3 GB | Host 源码 |
| ├─ `node_modules` | 2.1 GB | pnpm 安装产物 |
| └─ `.git` | 47 MB | depth 1 clone |
| **合计** | **≈ 5.4 GB** | |

- debug 的 321 MB 不代表交付体积；`ARCH.md` §18 记录的 release 产物是 **4.7 MB**（`opt-level="z"` + `lto="fat"` + `panic="abort"` + `strip`）。本机尚未做 release 构建验证。
- Host 的 2.1 GB `node_modules` 是上游固有成本；桌面形态的用户机需要 `dsh` 可用，因此这部分无法省略。

## 7. IO（自启动累计）

| 进程 | 读（`read_bytes`） | 写（`write_bytes`） | 说明 |
| --- | --- | --- | --- |
| `dsh-dioxus-ui` | 52 MB | 4 KB | 读 WebKit 库、字体、二进制自身 |
| `node` Host | 146 MB | 4.7 MB | 加载 36 个 tsx 模块与 typert 产物 |

- Host 的 stderr 被丢弃（`Stdio::null()`），stdout 只读启动行，因此应用侧几乎不产生持续磁盘写。
- 稳定期无持续 IO；数字随启动完成即停止增长。

## 8. 结论

1. **空闲成本约 520 MB 内存 + 2% 单核**，在 8 GB ARM 机器上属于轻量；其中 **2/3 内存来自 Host（Node），不是 Rust 前端**。
2. **Rust GUI 自身很省**：PSS 76 MB、私有脏页 27 MB、空闲 0.6%（平均）。
3. **WebKit 渲染进程比 Rust 主进程更吃内存**（88 MB vs 76 MB），这是「用系统 WebView 换掉自带内核」的必然结果，也是 D1 决策的代价，但比自带 Chromium 低一个数量级。
4. 启动时间的主要成本是**可消除的重复构建**（typert 20.7 s），不是运行时。

### 优化项（按收益排序）

| # | 措施 | 预期收益 | 代价 |
| --- | --- | --- | --- |
| 1 | `DSH_BIN` 改用带 `--no-typert` 的包装脚本 | 启动 −21 s（35 s → ~13 s） | 修改 Host 源码后需手动重新生成 typert |
| 2 | release 构建 | 二进制 321 MB → ~5 MB | 构建耗时（fat LTO），需实测 |
| 3 | 去掉 `Style {}` 的重复渲染 | 少量重绘开销 | 改 `main.rs` 的样式挂载方式 |
| 4 | 限制 Host 堆（`NODE_OPTIONS=--max-old-space-size`） | 可降 Node 峰值 | 长会话/大上下文易 OOM，不建议默认开 |

## 9. 复现命令

```bash
cd dsh-rust-dioxus/dsh-dioxus-ui
export PATH="$HOME/.nvm/versions/node/v24.20.0/bin:$HOME/.cargo/bin:$PATH"

# 1) 编译并启动（GUI 自拉 Host）
cargo build -p dsh-app
DSH_BIN=./scripts/dev-host.sh ./target/debug/dsh-dioxus-ui &

# 2) 取进程（顺序：GUI → WebKitNetwork → WebKitWeb → Host）
ps -o pid,ppid,nlwp,%cpu,rss,etime,comm -g "$(ps -o sid= -p <GUI_PID> | tr -d ' ')"

# 3) 内存
for p in <pids>; do grep -E '^(Rss|Pss):' /proc/$p/smaps_rollup; done

# 4) 空闲 CPU（T=10s）
declare -A t0; for p in <pids>; do t0[$p]=$(awk '{print $14+$15}' /proc/$p/stat); done
sleep 10
for p in <pids>; do t1=$(awk '{print $14+$15}' /proc/$p/stat); echo "$p $(( (t1-t0[$p])*10 ))%"; done
```

> CPU 公式：10 秒窗口内 `Δjiffies × 10` = 单核百分比（CLK_TCK=100）。窗口取 T 秒时系数为 `100/T`。

---

## 附录 A：本机补齐构建链时踩到的两处依赖

非设计问题，但换机器复现时一定会遇到，记在此处。

1. **`libssl-dev` 是硬依赖**（与 ARCH D11「不启用 TLS」不冲突）：`dioxus-desktop 0.8.0-alpha.3` 的 `tungstenite` 依赖写死了 `features = ["native-tls"]`，且不是 optional，
   `openssl-sys ← native-tls ← tungstenite ← dioxus-desktop`。关闭 `dioxus` 的 `devtools` feature 绕不过去，必须装系统 `libssl-dev`。
2. **SSH 需要经 HTTP 代理**（当直连被限速/拒绝时）：本机无 `nc`/`socat`，用 `scripts/ssh-via-http-proxy.sh`（python3 实现 CONNECT 隧道）：

   ```bash
   export DSH_PROXY_HOST=192.168.0.113 DSH_PROXY_PORT=7897
   export GIT_SSH_COMMAND="ssh -o ProxyCommand='$PWD/scripts/ssh-via-http-proxy.sh %h %p' -o BatchMode=yes"
   git clone --depth 1 git@github.com:ray2kwok/deepseek-harness.git
   ```

   注意：不要在 stdin EOF 时对代理连接做 `shutdown(SHUT_WR)`，多数 HTTP 代理会把它当成整条连接关闭，SSH 连 banner 都收不到。
