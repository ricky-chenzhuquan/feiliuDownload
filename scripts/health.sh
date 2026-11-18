#!/bin/bash
# 飞流下载 — 项目健康自检
# 用法: bash scripts/health.sh
# 输出: 各项状态 + 最终 PASS/FAIL 汇总。定时蒸馏轮开工前运行一次即可确认项目可发布状态。
ROOT="$(cd "$(dirname "$0")/.." && pwd)" && cd "$ROOT" || exit 1
PASS=0; FAIL=0

check() { # $1=名称 $2=命令
  if eval "$2" > /dev/null 2>&1; then
    echo "  ✓ $1"; PASS=$((PASS+1))
  else
    echo "  ✗ $1"; FAIL=$((FAIL+1))
  fi
}

echo "=== 飞流下载 健康自检 ==="

echo "[1] 前端构建"
check "tsc + vite build" "pnpm --dir app build"

echo "[2] Rust 编译"
check "cargo check 零错误零警告" "cargo check --manifest-path app/src-tauri/Cargo.toml 2>&1 | grep -qvE '^error|^warning' || ! cargo check --manifest-path app/src-tauri/Cargo.toml 2>&1 | grep -qE '^error|^warning'"

echo "[3] 单元测试（无网络）"
if cd app/src-tauri && cargo test --quiet > /tmp/feiliu_health_test.log 2>&1; then
  R=$(grep -oE "[0-9]+ passed" /tmp/feiliu_health_test.log | head -1)
  echo "  ✓ cargo test 通过（$R）"; PASS=$((PASS+1))
else
  echo "  ✗ cargo test 失败（详见 /tmp/feiliu_health_test.log）"; FAIL=$((FAIL+1))
fi
cd "$ROOT"

echo "[4] Git 同步"
LOCAL=$(git rev-parse HEAD 2>/dev/null)
REMOTE=$(git rev-parse origin/main 2>/dev/null)
if [ "$LOCAL" = "$REMOTE" ]; then echo "  ✓ 与 origin/main 同步 ($LOCAL)"; PASS=$((PASS+1))
else echo "  ✗ 本地与远端不同步（本地 $LOCAL ≠ 远端 $REMOTE）"; FAIL=$((FAIL+1)); fi

echo "[5] 工作树整洁"
if [ -z "$(git status --porcelain)" ]; then echo "  ✓ 无未提交变更"; PASS=$((PASS+1))
else echo "  ✗ 有未提交变更:"; git status --short | head -5; FAIL=$((FAIL+1)); fi

echo "[6] 敏感文件未入库"
check "无密钥文件泄漏" "! git ls-files | grep -E '\.private$|keystore$|\.pem$'"

echo "=== 结果: PASS=$PASS FAIL=$FAIL ==="
[ $FAIL -eq 0 ] && echo "✅ 项目处于可发布状态" || echo "❌ 存在 $FAIL 项问题，先修复再继续蒸馏"
exit $FAIL
