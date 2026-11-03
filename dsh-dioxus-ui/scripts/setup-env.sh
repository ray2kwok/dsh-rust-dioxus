#!/usr/bin/env bash
#
# dsh-dioxus-ui 环境检查与安装脚本
#
# 用于在任意 Linux / macOS 机器上一次判定「能否构建并运行本项目」，
# 并按需安装缺失依赖。默认只检查，不改动系统。
#
#   用法:
#     ./scripts/setup-env.sh                  # 只检查（默认）
#     ./scripts/setup-env.sh --install        # 安装缺失项（逐条确认）
#     ./scripts/setup-env.sh --install --yes  # 安装缺失项（不确认）
#     ./scripts/setup-env.sh --install --dry-run   # 只打印将执行的命令
#
#   选项:
#     --install        安装缺失项
#     --yes, -y        不逐条确认
#     --dry-run        与 --install 联用，只打印命令
#     --skip-node      跳过 Node / pnpm 检查与安装（只做桌面端构建）
#     --with-node      允许脚本安装/切换 Node（nvm / corepack）
#     --allow-root     允许以 root 运行（容器/CI 用；普通机器请勿用 sudo）
#     --help, -h       显示帮助
#
# 注意: 不要用 sudo 运行本脚本。系统包安装时脚本会自己对 apt/dnf 调用 sudo；
#       以 root 运行时 Rust/Node 工具链会装到 /root 下，检测结果与普通用户不一致。
#
# 退出码: 0 = 无阻塞项；1 = 存在阻塞项（--check 模式下用于 CI 门禁）
#
# 依赖事实（与 doc/ARCH.md §5 对应）:
#   - Rust >= 1.85（edition 2024），Dioxus 0.8.0-alpha.3
#   - Linux 需要 WebKitGTK 4.1 + libsoup3（wry 的系统 WebView）
#   - macOS 用系统 WKWebView，只需 Xcode 命令行工具
#   - 联调 Host 需要 Node >= 24.9 + pnpm 11.28.5（可选，仅开发期）
#
set -uo pipefail

# ---------------------------------------------------------------- 常量

readonly DIOXUS_VERSION="0.8.0-alpha.3"
# dioxus 框架本身 MSRV 1.85，但 dioxus-cli 0.8.0-alpha.3 要求 >= 1.93
readonly RUST_MIN="1.93.0"
readonly RUST_TOOLCHAIN="${DSH_RUST_TOOLCHAIN:-stable}"
readonly NODE_MIN="24.9.0"
readonly PNPM_MIN="11.0.0"
readonly DISK_MIN_GB=10

MODE="check"
ASSUME_YES=0
DRY_RUN=0
SKIP_NODE=0
WITH_NODE=0
ALLOW_ROOT=0
PATH_CHANGED=0

FAIL_COUNT=0
WARN_COUNT=0
declare -a INSTALL_ACTIONS=()

# ---------------------------------------------------------------- 输出

if [ -t 1 ] && [ "${NO_COLOR:-}" = "" ]; then
  C_OK=$'\033[32m'; C_WARN=$'\033[33m'; C_ERR=$'\033[31m'; C_DIM=$'\033[2m'; C_RST=$'\033[0m'
else
  C_OK=""; C_WARN=""; C_ERR=""; C_DIM=""; C_RST=""
fi

row() { # status name detail fix
  local status="$1" name="$2" detail="$3" fix="${4:-}"
  case "$status" in
    ok)   printf '  %s✓%s %-22s %s\n' "$C_OK" "$C_RST" "$name" "$detail" ;;
    warn) printf '  %s!%s %-22s %s\n' "$C_WARN" "$C_RST" "$name" "$detail"
          [ -n "$fix" ] && printf '      %s↳ %s%s\n' "$C_DIM" "$fix" "$C_RST"
          WARN_COUNT=$((WARN_COUNT + 1)) ;;
    fail) printf '  %s✗%s %-22s %s\n' "$C_ERR" "$C_RST" "$name" "$detail"
          [ -n "$fix" ] && printf '      %s↳ %s%s\n' "$C_DIM" "$fix" "$C_RST"
          FAIL_COUNT=$((FAIL_COUNT + 1)) ;;
  esac
}

SECTION_COUNT=0
section() {
  SECTION_COUNT=$((SECTION_COUNT + 1))
  if [ "$SECTION_COUNT" -eq 1 ]; then printf '%s\n' "$1"; else printf '\n%s\n' "$1"; fi
}
info()    { printf '  %s%s%s\n' "$C_DIM" "$1" "$C_RST"; }

die() { printf '%s错误:%s %s\n' "$C_ERR" "$C_RST" "$1" >&2; exit 2; }

usage() { sed -n '3,30p' "$0" | sed 's/^# \{0,1\}//'; exit 0; }

# ---------------------------------------------------------------- 工具

version_ge() { # a >= b ?
  [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -n1)" = "$2" ]
}

have() { command -v "$1" >/dev/null 2>&1; }

sudo_prefix() {
  if [ "$(id -u)" -eq 0 ]; then
    SUDO=()
  elif have sudo; then
    SUDO=(sudo)
  else
    SUDO=()
  fi
}

# 执行一条安装命令（数组形式传入）；遵循 --dry-run / --yes
run_install() {
  # 命令不存在时不要抛 "command not found"，给出可读原因（常见于缺 cargo 就装 dx）
  if ! have "$1"; then
    printf '  %s跳过%s：找不到命令 %s%s\n' "$C_WARN" "$C_RST" "'$1'" ""
    return 1
  fi
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '  %s[dry-run]%s %s\n' "$C_DIM" "$C_RST" "$*"
    return 0
  fi
  if [ "$ASSUME_YES" -eq 0 ]; then
    printf '  执行: %s\n  [y/N] ' "$*"
    local answer=""
    read -r answer || true
    case "$answer" in
      y|Y|yes|YES) ;;
      *) printf '  跳过\n'; return 1 ;;
    esac
  fi
  printf '  运行: %s\n' "$*"
  "$@" || { printf '  %s失败%s\n' "$C_ERR" "$C_RST"; return 1; }
}

# ---------------------------------------------------------------- 参数

while [ $# -gt 0 ]; do
  case "$1" in
    --check)      MODE="check" ;;
    --install)    MODE="install" ;;
    --yes|-y)     ASSUME_YES=1 ;;
    --dry-run)    DRY_RUN=1 ;;
    --skip-node)  SKIP_NODE=1 ;;
    --with-node)  WITH_NODE=1 ;;
    --allow-root) ALLOW_ROOT=1 ;;
    --help|-h)    usage ;;
    *)            die "未知参数: $1（--help 查看用法）" ;;
  esac
  shift
done

# ---------------------------------------------------------------- 平台探测

detect_platform() {
  OS="$(uname -s)"
  DISTRO="unknown"
  DISTRO_LIKE=""
  PKG_MANAGER="none"

  if [ "$OS" = "Darwin" ]; then
    DISTRO="macos"
  elif [ -r /etc/os-release ]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    DISTRO="${ID:-unknown}"
    DISTRO_LIKE="${ID_LIKE:-}"
  fi

  case "$DISTRO" in
    macos) : ;;
    ubuntu|debian|linuxmint|pop|raspbian) PKG_MANAGER="apt" ;;
    fedora|rhel|centos|rocky|almalinux)   PKG_MANAGER="dnf" ;;
    arch|manjaro|endeavouros)             PKG_MANAGER="pacman" ;;
    opensuse*|sles)                       PKG_MANAGER="zypper" ;;
    *)
      case " $DISTRO_LIKE " in
        *debian*|*ubuntu*) PKG_MANAGER="apt" ;;
        *fedora*|*rhel*)   PKG_MANAGER="dnf" ;;
        *arch*)            PKG_MANAGER="pacman" ;;
      esac
      ;;
  esac

  printf 'dsh-dioxus-ui 环境检查  %s(%s / %s, 包管理器: %s)%s\n\n' \
    "$C_DIM" "$OS" "$DISTRO" "$PKG_MANAGER" "$C_RST"
}

# 防止用 sudo 运行导致工具链装到 /root、检测结果失真
guard_root() {
  [ "$(id -u)" -eq 0 ] || return 0
  [ "$ALLOW_ROOT" -eq 1 ] && return 0
  if [ -n "${SUDO_USER:-}" ]; then
    printf '\n%s请不要用 sudo 运行本脚本。%s\n' "$C_ERR" "$C_RST"
    printf '  以 root 检测会把 %s 的 Rust/Node 工具链当成“未安装”，并可能装到 /root 下。\n' "$SUDO_USER"
    printf '  正确用法:\n    ./scripts/setup-env.sh --install     # 脚本会在装系统包时自行调用 sudo\n'
    printf '  若确需 root（容器/CI），加 --allow-root。\n'
    exit 2
  fi
  info "以 root 运行且无 SUDO_USER（容器环境）；工具链将安装到 /root"
}

# ---------------------------------------------------------------- 检查项

check_rust() {
  section "Rust 工具链（要求 >= ${RUST_MIN}，edition 2024；该下限由 dioxus-cli 决定）"

  if ! have rustc; then
    row fail "rustc" "未安装" "安装 rustup: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    INSTALL_ACTIONS+=("rustup")
    return
  fi

  local ver
  ver="$(rustc --version | awk '{print $2}')"
  if version_ge "$ver" "$RUST_MIN"; then
    row ok "rustc" "$ver"
  else
    row fail "rustc" "$ver（低于 $RUST_MIN）" "rustup toolchain install ${RUST_TOOLCHAIN} && rustup default ${RUST_TOOLCHAIN}"
    INSTALL_ACTIONS+=("rustc-toolchain")
  fi

  if have cargo; then
    local cver
    cver="$(cargo --version | awk '{print $2}')"
    if version_ge "$cver" "$RUST_MIN"; then
      row ok "cargo" "$cver"
    else
      row warn "cargo" "$cver（与 rustc 同步升级）"
    fi
  else
    row fail "cargo" "未安装" "随 rustup 一起安装"
  fi

  # rust-toolchain.toml 与已安装工具链的一致性
  local tc_file
  for tc_file in ./rust-toolchain.toml ./dsh-dioxus-ui/rust-toolchain.toml; do
    [ -f "$tc_file" ] || continue
    local channel
    channel="$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$tc_file" | head -n1)"
    if [ -n "$channel" ] && have rustup; then
      if rustup toolchain list | grep -q "^${channel}"; then
        row ok "toolchain pin" "$channel（已安装）"
      else
        row warn "toolchain pin" "$channel（未安装，首次构建会自动下载）" "rustup toolchain install $channel"
      fi
    fi
    break
  done

  if have rustup && [ "$OS" = "Linux" ]; then
    if rustup target list --installed 2>/dev/null | grep -q wasm32-unknown-unknown; then
      info "wasm32-unknown-unknown 已安装（浏览器形态需要，桌面形态不需要）"
    fi
  fi
}

check_system_libs() {
  section "系统 WebView 依赖（dioxus-desktop / wry）"

  if [ "$OS" = "Darwin" ]; then
    if xcode-select -p >/dev/null 2>&1; then
      row ok "Xcode CLT" "$(xcode-select -p)"
    else
      row fail "Xcode CLT" "未安装" "xcode-select --install"
      INSTALL_ACTIONS+=("xcode-clt")
    fi
    info "macOS 使用系统 WKWebView，无需额外 WebView 库"
    return
  fi

  if ! have pkg-config; then
    row fail "pkg-config" "未安装" "$(pkg_hint pkg-config)"
    INSTALL_ACTIONS+=("sysdeps")
    return
  fi

  local missing=()
  local pkg ver
  # wry 0.57 在 Linux 上依赖 WebKitGTK 4.1 + libsoup3
  for pkg in webkit2gtk-4.1 javascriptcoregtk-4.1 libsoup-3.0 gtk+-3.0 glib-2.0; do
    if ver="$(pkg-config --modversion "$pkg" 2>/dev/null)"; then
      row ok "$pkg" "$ver"
    else
      row fail "$pkg" "缺失" "$(pkg_hint "$pkg")"
      missing+=("$pkg")
    fi
  done

  if [ ${#missing[@]} -gt 0 ]; then
    INSTALL_ACTIONS+=("sysdeps")
  fi

  if ! have cc; then
    row fail "cc" "未安装（C 编译器）" "$(pkg_hint build-essential)"
    INSTALL_ACTIONS+=("sysdeps")
  fi
}

pkg_hint() { # 给单个依赖打印本发行版的安装方式
  case "$PKG_MANAGER" in
    apt)    printf "sudo apt-get install -y %s" "$(apt_name_for "$1")" ;;
    dnf)    printf "sudo dnf install -y %s" "$(dnf_name_for "$1")" ;;
    pacman) printf "sudo pacman -S --needed %s" "$(pacman_name_for "$1")" ;;
    zypper) printf "sudo zypper install -y %s" "$(zypper_name_for "$1")" ;;
    *)      printf "请用系统包管理器安装 %s" "$1" ;;
  esac
}

apt_name_for() {
  case "$1" in
    webkit2gtk-4.1)          echo libwebkit2gtk-4.1-dev ;;
    javascriptcoregtk-4.1)   echo libjavascriptcoregtk-4.1-dev ;;
    libsoup-3.0)             echo libsoup-3.0-dev ;;
    gtk+-3.0)                echo libgtk-3-dev ;;
    glib-2.0)                echo libglib2.0-dev ;;
    pkg-config)              echo pkg-config ;;
    build-essential|cc)      echo build-essential ;;
    *)                       echo "$1" ;;
  esac
}

dnf_name_for() {
  case "$1" in
    webkit2gtk-4.1)          echo webkit2gtk4.1-devel ;;
    javascriptcoregtk-4.1)   echo javascriptcoregtk4.1-devel ;;
    libsoup-3.0)             echo libsoup3-devel ;;
    gtk+-3.0)                echo gtk3-devel ;;
    glib-2.0)                echo glib2-devel ;;
    pkg-config)              echo pkgconf-pkg-config ;;
    build-essential|cc)      echo gcc gcc-c++ make ;;
    *)                       echo "$1" ;;
  esac
}

pacman_name_for() {
  case "$1" in
    build-essential|cc) echo base-devel ;;
    pkg-config)         echo pkgconf ;;
    *)                  echo "$1" ;;
  esac
}

zypper_name_for() {
  case "$1" in
    webkit2gtk-4.1)        echo libwebkit2gtk-4_1-0-devel ;;
    javascriptcoregtk-4.1) echo libjavascriptcoregtk-4_1-0-devel ;;
    libsoup-3.0)           echo libsoup-3_0-devel ;;
    gtk+-3.0)              echo gtk3-devel ;;
    glib-2.0)              echo glib2-devel ;;
    pkg-config)            echo pkg-config ;;
    build-essential|cc)    echo gcc gcc-c++ make ;;
    *)                     echo "$1" ;;
  esac
}

check_dx() {
  section "Dioxus CLI（建议与框架同版本 ${DIOXUS_VERSION}）"

  if ! have dx; then
    row fail "dx" "未安装" "cargo install dioxus-cli --version ${DIOXUS_VERSION} --locked"
    INSTALL_ACTIONS+=("dx")
    return
  fi

  local out ver
  out="$(dx --version 2>&1 | head -n1)"
  ver="$(printf '%s' "$out" | grep -oE '[0-9]+\.[0-9]+\.[0-9]+(-[a-z0-9.]+)?' | head -n1)"
  if [ -z "$ver" ]; then
    row warn "dx" "$out（无法解析版本）"
    return
  fi
  case "$ver" in
    "$DIOXUS_VERSION") row ok "dx" "$ver" ;;
    0.8.*)             row warn "dx" "$ver（建议精确匹配 $DIOXUS_VERSION）" "cargo install dioxus-cli --version ${DIOXUS_VERSION} --locked --force" ;;
    *)                 row fail "dx" "$ver（与 $DIOXUS_VERSION 不匹配）" "cargo install dioxus-cli --version ${DIOXUS_VERSION} --locked --force"
                       INSTALL_ACTIONS+=("dx") ;;
  esac
}

check_node() {
  if [ "$SKIP_NODE" -eq 1 ]; then
    section "Node / pnpm（已按 --skip-node 跳过）"
    info "仅构建桌面端时不需要；联调 Host 需要 Node >= ${NODE_MIN} + pnpm"
    return
  fi

  section "Node / pnpm（联调 Host 需要，可选）"

  if ! have node; then
    local nvm_hint="安装 Node ${NODE_MIN}+（建议 nvm/fnm）：nvm install 24 && nvm use 24"
    if [ -s "${NVM_DIR:-$HOME/.nvm}/nvm.sh" ]; then
      nvm_hint="检测到 nvm：source ~/.nvm/nvm.sh && nvm install 24 && nvm use 24"
    fi
    row warn "node" "不在 PATH" "$nvm_hint"
    if [ "$WITH_NODE" -eq 1 ] && [ -s "${NVM_DIR:-$HOME/.nvm}/nvm.sh" ]; then
      INSTALL_ACTIONS+=("node-nvm")
    fi
  else
    local nver
    nver="$(node -v | sed 's/^v//')"
    if version_ge "$nver" "$NODE_MIN"; then
      row ok "node" "$nver"
    else
      row warn "node" "$nver（低于 $NODE_MIN）" "nvm install 24 && nvm use 24"
    fi
  fi

  if have pnpm; then
    local pver
    pver="$(pnpm --version 2>/dev/null | head -n1)"
    if version_ge "${pver:-0}" "$PNPM_MIN"; then
      row ok "pnpm" "$pver"
    else
      row warn "pnpm" "${pver:-未知}（低于 $PNPM_MIN）" "corepack prepare pnpm@11.28.5 --activate"
    fi
  elif have corepack; then
    row warn "pnpm" "未激活（corepack 可用）" "corepack prepare pnpm@11.28.5 --activate"
    [ "$WITH_NODE" -eq 1 ] && INSTALL_ACTIONS+=("pnpm-corepack")
  else
    row warn "pnpm" "未安装" "corepack prepare pnpm@11.28.5 --activate（Node 24 自带 corepack）"
  fi

  if have dsh; then
    row ok "dsh" "$(command -v dsh)"
  else
    row warn "dsh" "不在 PATH" "桌面应用运行期需要它；联调前请先安装 deepseek-harness"
  fi
}

check_runtime() {
  section "运行环境"

  if [ "$OS" = "Darwin" ]; then
    row ok "display" "macOS 原生"
  elif [ -n "${WAYLAND_DISPLAY:-}" ]; then
    row ok "display" "Wayland (${WAYLAND_DISPLAY})"
  elif [ -n "${DISPLAY:-}" ]; then
    row ok "display" "X11 (${DISPLAY})"
  else
    row warn "display" "无 DISPLAY / WAYLAND_DISPLAY" "桌面窗口无法打开；CI 中可用 Xvfb"
  fi

  local free_gb
  free_gb="$(df -Pk . 2>/dev/null | awk 'NR==2 {printf "%d", $4/1024/1024}')"
  if [ -n "${free_gb:-}" ] && [ "$free_gb" -ge "$DISK_MIN_GB" ]; then
    row ok "disk free" "${free_gb} GB"
  else
    row warn "disk free" "${free_gb:-未知} GB（建议 >= ${DISK_MIN_GB} GB 用于 Cargo target）"
  fi
}

check_network() {
  section "网络（首次构建 / 安装需要）"

  if ! have curl; then
    row warn "curl" "未安装" "$(pkg_hint curl)"
    return
  fi

  local url code
  for url in https://static.rust-lang.org/dist/channel-rust-stable.toml https://index.crates.io/config.json; do
    code="$(curl -s -o /dev/null -m 8 -w '%{http_code}' "$url" 2>/dev/null || echo 000)"
    case "$code" in
      200|301|302|403) row ok "$(printf '%s' "$url" | cut -d/ -f3)" "HTTP $code" ;;
      *)               row warn "$(printf '%s' "$url" | cut -d/ -f3)" "不可达（HTTP $code）" "检查代理：export https_proxy=http://127.0.0.1:7890" ;;
    esac
  done
}

# ---------------------------------------------------------------- 安装

sys_install_cmd() { # 输出本发行版的整包安装命令
  case "$PKG_MANAGER" in
    apt)
      printf '%s\n' "apt-get update"
      printf '%s\n' "apt-get install -y libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libgtk-3-dev libglib2.0-dev pkg-config build-essential"
      ;;
    dnf)
      printf '%s\n' "dnf install -y webkit2gtk4.1-devel libsoup3-devel javascriptcoregtk4.1-devel gtk3-devel glib2-devel pkgconf-pkg-config gcc gcc-c++ make"
      ;;
    pacman)
      printf '%s\n' "pacman -S --needed --noconfirm webkit2gtk-4.1 libsoup3 gtk3 base-devel pkgconf"
      ;;
    zypper)
      printf '%s\n' "zypper --non-interactive install libwebkit2gtk-4_1-0-devel libsoup-3_0-devel libjavascriptcoregtk-4_1-0-devel gtk3-devel glib2-devel pkg-config gcc gcc-c++ make"
      ;;
    *) return 1 ;;
  esac
}

do_install() {
  section "安装缺失项"

  if [ ${#INSTALL_ACTIONS[@]} -eq 0 ]; then
    info "没有需要安装的项"
    return
  fi

  sudo_prefix
  local action done_any=0

  # 系统库（一次装齐）
  for action in "${INSTALL_ACTIONS[@]}"; do
    if [ "$action" = "sysdeps" ]; then
      local cmd
      if ! cmd="$(sys_install_cmd)"; then
        row warn "sysdeps" "未识别的包管理器（$PKG_MANAGER）" "请手动安装 WebKitGTK 4.1 / libsoup3 开发包"
        continue
      fi
      printf '  将以 root 安装 WebKitGTK 4.1 + libsoup3 + 构建工具\n'
      while IFS= read -r line; do
        [ -n "$line" ] || continue
        # 拆成词，前缀 sudo
        read -r -a argv <<<"$line"
        run_install "${SUDO[@]}" "${argv[@]}" && done_any=1
      done <<<"$cmd"
      break
    fi
  done

  for action in "${INSTALL_ACTIONS[@]}"; do
    case "$action" in
      rustc-toolchain)
        run_install rustup toolchain install "$RUST_TOOLCHAIN" \
          && run_install rustup default "$RUST_TOOLCHAIN" && done_any=1 ;;
      rustup)
        if ! have curl; then
          row warn "rustup" "无法自动安装（缺 curl）" "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"
          continue
        fi
        printf '  安装 rustup（装入 %s，并设置默认工具链 %s）\n' "${CARGO_HOME:-$HOME/.cargo}" "$RUST_MIN"
        if [ "$DRY_RUN" -eq 1 ]; then
          printf '  %s[dry-run]%s curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain %s\n' \
            "$C_DIM" "$C_RST" "$RUST_TOOLCHAIN"
        else
          if curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
               | sh -s -- -y --profile minimal --default-toolchain "$RUST_TOOLCHAIN"; then
            # 当前进程补 PATH，后续同进程的 dx 安装与复检才能看到 cargo
            export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
            PATH_CHANGED=1
            done_any=1
          else
            printf '  %srustup 安装失败%s\n' "$C_ERR" "$C_RST"
          fi
        fi
        ;;
      dx)
        run_install cargo install dioxus-cli --version "$DIOXUS_VERSION" --locked && done_any=1 ;;
      xcode-clt)
        run_install xcode-select --install || true ;;
      pnpm-corepack)
        run_install corepack prepare pnpm@11.28.5 --activate && done_any=1 ;;
      node-nvm)
        printf '  通过 nvm 安装 Node 24（子 shell 内生效）\n'
        if [ "$DRY_RUN" -eq 1 ]; then
          printf '  %s[dry-run]%s nvm install 24\n' "$C_DIM" "$C_RST"
        else
          bash -lc 'source "$HOME/.nvm/nvm.sh" && nvm install 24 && nvm use 24' && done_any=1
        fi
        ;;
    esac
  done

  if [ "$DRY_RUN" -eq 1 ]; then
    printf '\n  %s(dry-run: 未做任何修改)%s\n' "$C_DIM" "$C_RST"
    return 0
  fi

  if [ "$PATH_CHANGED" -eq 1 ]; then
    printf '\n  提示: rustup 已修改 PATH，当前 shell 需执行 %s 或重开终端。\n' "source \"\$HOME/.cargo/env\""
  fi

  if [ "$done_any" -eq 1 ]; then
    printf '\n重新检查:\n'
    if [ "$SKIP_NODE" -eq 1 ]; then
      "$0" --check --skip-node
    else
      "$0" --check
    fi
    exit $?
  fi
}

# ---------------------------------------------------------------- 主流程

detect_platform
guard_root
check_rust
check_system_libs
check_dx
check_node
check_runtime
check_network

section "结论"
if [ "$FAIL_COUNT" -eq 0 ]; then
  printf '  %s可构建%s' "$C_OK" "$C_RST"
  [ "$WARN_COUNT" -gt 0 ] && printf '（有 %d 个警告，见上）' "$WARN_COUNT"
  printf '\n\n  下一步:\n    dx build --platform desktop --release\n'
  [ "$MODE" = "install" ] && do_install
  exit 0
fi

printf '  %s存在 %d 个阻塞项%s' "$C_ERR" "$FAIL_COUNT" "$C_RST"
[ "$WARN_COUNT" -gt 0 ] && printf '，另有 %d 个警告' "$WARN_COUNT"
printf '\n'

if [ "$MODE" = "install" ]; then
  do_install
else
  printf '\n  自动安装: %s\n' "$0 --install"
fi

exit 1
