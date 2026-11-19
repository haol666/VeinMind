#!/usr/bin/env bash
#
# VeinMind 一键安装依赖并启动开发模式
# 支持 macOS / Linux（需已安装 Rust 工具链）
set -euo pipefail

cd "$(dirname "$0")"

log()  { printf '\033[1;32m[VeinMind]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[VeinMind]\033[0m %s\n' "$*"; }
die()  { printf '\033[1;31m[VeinMind]\033[0m %s\n' "$*" >&2; exit 1; }

# ---------- 1. 检查 Rust 工具链 ----------
if ! command -v cargo >/dev/null 2>&1; then
  cat >&2 <<'EOF'
[VeinMind] 未检测到 Rust 工具链，请先安装：
  推荐使用 rustup：   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  安装完成后重新打开终端，再执行本脚本。
EOF
  exit 1
fi
log "检测到 Rust: $(cargo --version | head -1)"

# ---------- 2. 检查 / 安装 cargo-tauri CLI ----------
# 说明：Tauri CLI 的发布包名为 tauri-cli，其产出可执行文件叫 cargo-tauri。
# 需与项目所用 Tauri 主版本匹配（本项目为 v1.6）。
if command -v cargo-tauri >/dev/null 2>&1; then
  log "已安装 cargo-tauri: $(cargo-tauri --version)"
elif cargo tauri --version >/dev/null 2>&1; then
  log "已安装 Tauri CLI: $(cargo tauri --version)"
else
  log "未检测到 cargo-tauri，开始安装 tauri-cli v1.6（耗时较长，请耐心等待）..."
  cargo install tauri-cli --version "1.6.6" --locked
  log "cargo-tauri 安装完成"
fi

# ---------- 3. 编译检查 ----------
log "编译检查中..."
cargo check

# ---------- 4. 启动开发模式 ----------
log "启动 VeinMind 开发模式..."
exec cargo tauri dev
