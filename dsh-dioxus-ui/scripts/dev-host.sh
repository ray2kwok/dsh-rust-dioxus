#!/usr/bin/env bash
#
# 以「源码形态」启动 deepseek-harness Host，供 dsh-dioxus-ui 联调。
#
# 为什么需要这个脚本：
#   1. Host 需要 build 产物 `lib/typert.host.js`（Typert 描述符），否则启动即报
#      "typert contributor(s) failed to register"。生成它比整仓 build 快得多。
#   2. `dsh web` 默认要求前端 client bundle（lib/client.js）存在，而 Rust 前端
#      根本不加载它们；用 patch 关闭 `modules` 插件即可绕过整条前端构建链。
#   3. `--patch` 必须紧跟子命令，放在 `--port` 之后会被判为 unknown option。
#
# 用法:
#   ./scripts/dev-host.sh              # 启动并打印带 token 的地址
#   DSH_REPO=/path/to/deepseek-harness ./scripts/dev-host.sh
#   ./scripts/dev-host.sh --no-typert  # 跳过描述符生成（已生成过时加快启动）
#
# 退出：Ctrl-C 终止 Host。

set -euo pipefail

DSH_REPO="${DSH_REPO:-$(cd "$(dirname "$0")/../../deepseek-harness" 2>/dev/null && pwd || true)}"
PORT="${DSH_PORT:-0}"
GEN_TYPERT=1

for arg in "$@"; do
  case "$arg" in
    --no-typert) GEN_TYPERT=0 ;;
    --help|-h)   sed -n '3,22p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    # 允许直接作为 DSH_BIN 被客户端调用（客户端会传 web --no-open --port 0），
    # 这些参数由本脚本自行拼装，这里只做忽略。
    web|--no-open|--port|0|[0-9]*) ;;
    *)           echo "未知参数: $arg" >&2; exit 2 ;;
  esac
done

if [ -z "${DSH_REPO}" ] || [ ! -d "${DSH_REPO}/apps/cli" ]; then
  echo "找不到 deepseek-harness 仓库，请设置 DSH_REPO" >&2
  exit 2
fi

# Node 24+：优先用已有的 PATH，其次尝试 nvm 默认目录
if ! command -v node >/dev/null 2>&1; then
  for candidate in "$HOME"/.nvm/versions/node/v24*/bin; do
    [ -d "$candidate" ] && PATH="$candidate:$PATH" && break
  done
fi
if ! command -v node >/dev/null 2>&1; then
  echo "未找到 node（需 >= 24.9）" >&2
  exit 2
fi

cd "${DSH_REPO}"

if [ ! -d node_modules ]; then
  echo "== 安装依赖（首次）=="
  corepack enable pnpm 2>/dev/null || true
  corepack pnpm install
fi

if [ "${GEN_TYPERT}" -eq 1 ]; then
  echo "== 生成 Typert 描述符 =="
  corepack pnpm exec tsx scripts/generate-typert.ts
fi

# 关闭 client modules：Rust 前端不加载这些 bundle
PATCH_FILE="$(mktemp -t dsh-patch-XXXXXX.yml)"
trap 'rm -f "${PATCH_FILE}"' EXIT
cat >"${PATCH_FILE}" <<'YAML'
- id: modules
  disabled: true
YAML

echo "== 启动 Host =="
exec node --import tsx/esm apps/cli/src/bin.ts web \
  --patch "${PATCH_FILE}" \
  --no-open \
  --port "${PORT}"
