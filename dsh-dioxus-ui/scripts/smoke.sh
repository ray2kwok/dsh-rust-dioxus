#!/usr/bin/env bash
#
# 端到端冒烟：不弹窗口截图，而是用日志断言整条启动链路。
#
# 断言序列（缺一即失败）：
#   1. Host 握手完成
#   2. $events ready（拿到 generation）
#   3. session/list 完成
#   4. follow 已就绪（会话快照进了 SessionView）
#
# 用法:
#   ./scripts/smoke.sh               # 跑测试 + 冒烟
#   ./scripts/smoke.sh --no-test     # 跳过 cargo test
#   DSH_REPO=... ./scripts/smoke.sh  # 指定 Host 仓库
#
# 需要: node >= 24、deepseek-harness 已 pnpm install（见 dev-host.sh）

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUN_TESTS=1
for arg in "$@"; do
  case "$arg" in
    --no-test) RUN_TESTS=0 ;;
    --help|-h) sed -n '3,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "未知参数: $arg" >&2; exit 2 ;;
  esac
done

cd "${ROOT}"

fail() { printf '\033[31m✗ %s\033[0m\n' "$1"; exit 1; }
ok()   { printf '\033[32m✓ %s\033[0m\n' "$1"; }

if [ "${RUN_TESTS}" -eq 1 ]; then
  echo "== cargo test =="
  cargo test --workspace --quiet || fail "单元测试失败"
  ok "单元测试通过"
fi

echo "== 构建 debug 二进制 =="
cargo build -p dsh-app --quiet || fail "编译失败"

LOG="$(mktemp -t dsh-smoke-XXXXXX.log)"
trap 'rm -f "${LOG}"' EXIT

echo "== 启动 GUI（最长 40s）=="
DSH_BIN="${ROOT}/scripts/dev-host.sh" timeout 40 ./target/debug/dsh-dioxus-ui >"${LOG}" 2>&1
STATUS=$?
# 124 = 被 timeout 结束，属于预期（应用常驻）
if [ "${STATUS}" -ne 124 ] && [ "${STATUS}" -ne 0 ]; then
  tail -20 "${LOG}"
  fail "进程异常退出（status=${STATUS}）"
fi

CLEAN="$(sed 's/\x1b\[[0-9;]*m//g' "${LOG}")"

check() {
  local pattern="$1" label="$2"
  if grep -q "${pattern}" <<<"${CLEAN}"; then
    ok "${label}"
  else
    echo "--- 日志尾部 ---"
    tail -20 "${CLEAN}"
    fail "${label}（未匹配: ${pattern}）"
  fi
}

check "Host 握手完成" "1/4 Host 握手"
check "\$events ready" "2/4 \$events generation"
check "session/list 完成" "3/4 会话列表"
check "follow 已就绪" "4/4 会话快照"

printf '\n\033[32m冒烟通过\033[0m\n'
