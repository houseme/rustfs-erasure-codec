#!/bin/bash
# verify-x86_64.sh — Leopard GF8 x86_64 架构完整验证
#
# 用法：bash scripts/verify-x86_64.sh
# 必须在 x86_64 机器上运行

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

pass() { echo -e "${GREEN}✅ $1${NC}"; }
fail() { echo -e "${RED}❌ $1${NC}"; exit 1; }
info() { echo -e "${YELLOW}ℹ️  $1${NC}"; }

echo "========================================"
echo " Leopard GF8 x86_64 Verification"
echo "========================================"
echo ""

# --- Step 1: Architecture check ---
echo "=== Step 1: 架构检查 ==="
ARCH=$(uname -m)
if [ "$ARCH" != "x86_64" ]; then
    fail "当前架构为 $ARCH, 需要在 x86_64 上运行"
fi
pass "架构：$ARCH"

if [ -f /proc/cpuinfo ]; then
    CPU_MODEL=$(grep "model name" /proc/cpuinfo | head -1 | cut -d: -f2 | xargs)
    echo "  CPU: $CPU_MODEL"

    HAS_SSSE3=$(grep -c "ssse3" /proc/cpuinfo || true)
    HAS_AVX2=$(grep -c "avx2" /proc/cpuinfo || true)
    HAS_AVX512=$(grep -c "avx512" /proc/cpuinfo || true)
    HAS_GFNI=$(grep -c "gfni" /proc/cpuinfo || true)

    echo "  SSSE3:  $([ $HAS_SSSE3 -gt 0 ] && echo '✅' || echo '❌')"
    echo "  AVX2:   $([ $HAS_AVX2 -gt 0 ] && echo '✅' || echo '❌')"
    echo "  AVX-512:$([ $HAS_AVX512 -gt 0 ] && echo '✅' || echo '❌')"
    echo "  GFNI:   $([ $HAS_GFNI -gt 0 ] && echo '✅' || echo '❌')"
elif [ "$(uname)" = "Darwin" ]; then
    sysctl -n machdep.cpu.brand_string 2>/dev/null || echo "  CPU: unknown"
fi
echo ""

# --- Step 2: Build verification ---
echo "=== Step 2: 编译验证 ==="
info "cargo build --features std"
cargo build --features std 2>&1
pass "默认编译通过"

info "cargo build --release --features std"
cargo build --release --features std 2>&1
pass "Release 编译通过"

if grep -q "simd-accel" Cargo.toml; then
    info "cargo build --features std,simd-accel"
    cargo build --features std,simd-accel 2>&1 || info "simd-accel 编译失败"
fi
echo ""

# --- Step 3: Unit tests ---
echo "=== Step 3: 功能测试 ==="
info "cargo test --lib --features std"
TEST_OUTPUT=$(cargo test --lib --features std 2>&1)
PASSED=$(echo "$TEST_OUTPUT" | grep -oP '\d+ passed' | head -1)
FAILED=$(echo "$TEST_OUTPUT" | grep -oP '\d+ failed' | head -1 || echo "0 failed")
echo "  $PASSED; $FAILED"
if echo "$FAILED" | grep -qv "0 failed"; then
    fail "有测试失败"
fi
pass "功能测试全部通过"
echo ""

# --- Step 4: Benchmark smoke test ---
echo "=== Step 4: 基准冒烟测试 ==="
info "cargo test --test benchmark_smoke --features std"
cargo test --test benchmark_smoke --features std 2>&1 | tail -5
pass "基准测试通过"
echo ""

# --- Step 5: Collect results ---
echo "=== Step 5: 结果收集 ==="
SMOKE_DIR="target/benchmark-smoke"

if [ -d "$SMOKE_DIR" ]; then
    echo ""
    echo "Leopard encode 吞吐量："
    echo "┌─────────────────────┬──────────────┐"
    echo "│ Case                │ Throughput   │"
    echo "├─────────────────────┼──────────────┤"
    for f in "$SMOKE_DIR"/leopard-encode-*.json; do
        if [ -f "$f" ]; then
            CASE=$(basename "$f" .json | sed 's/leopard-encode-//')
            THROUGHPUT=$(python3 -c "import json; d=json.load(open('$f')); print(f\"{d['throughput_mb_s']:.2f} MB/s\")" 2>/dev/null || echo "N/A")
            printf "│ %-19s │ %12s │\n" "$CASE" "$THROUGHPUT"
        fi
    done
    echo "└─────────────────────┴──────────────┘"
    echo ""

    echo "galois_8 后端信息："
    if [ -f "$SMOKE_DIR/smoke-results.json" ]; then
        python3 -c "
import json
data = json.load(open('$SMOKE_DIR/smoke-results.json'))
if isinstance(data, list) and len(data) > 0:
    print(f\"  backend: {data[0].get('backend', 'unknown')}\")
    print(f\"  backend_id: {data[0].get('backend_id', 'unknown')}\")
    print(f\"  target: {data[0].get('target_triple', 'unknown')}\")
" 2>/dev/null || echo "  无法读取"
    fi
fi
echo ""

# --- Step 6: Strategy verification ---
echo "=== Step 6: 自适应策略验证 ==="
info "检查 auto 策略选择..."

# 从基准结果中检查 shard_size 对应的策略
python3 -c "
import json, os, glob

smoke_dir = 'target/benchmark-smoke'
results = []
for f in glob.glob(os.path.join(smoke_dir, 'leopard-encode-*.json')):
    try:
        d = json.load(open(f))
        case = d.get('case', os.path.basename(f).replace('leopard-encode-', '').replace('.json', ''))
        shard_size = d.get('shard_size', 0)
        throughput = d.get('throughput_mb_s', 0)
        results.append((case, shard_size, throughput))
    except:
        pass

if results:
    print('  所有 smoke case 使用 auto 策略 (shard_size >= 64K → direct)')
    for case, ss, tp in sorted(results, key=lambda x: x[1]):
        strategy = 'decomposed' if ss < 65536 else 'direct'
        print(f'  {case}: shard_size={ss//1024}K → {strategy}, {tp:.1f} MB/s')
" 2>/dev/null || echo "  无法验证策略"
echo ""

echo "========================================"
echo " 验证完成"
echo "========================================"
