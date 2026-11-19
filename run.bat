@echo off
REM ============================================================
REM  VeinMind 一键安装依赖并启动开发模式 (Windows)
REM  前置要求：已安装 Rust 工具链
REM ============================================================
setlocal enabledelayedexpansion
cd /d "%~dp0"

echo [VeinMind] 检测 Rust 工具链...
where cargo >nul 2>nul
if errorlevel 1 (
  echo [VeinMind] 未检测到 Rust，请先安装：https://rustup.rs/
  pause
  exit /b 1
)

echo [VeinMind] 检测 cargo-tauri CLI...
where cargo-tauri >nul 2>nul
if errorlevel 1 (
  REM 说明：Tauri CLI 发布包名为 tauri-cli，可执行文件叫 cargo-tauri
  echo [VeinMind] 未检测到 cargo-tauri，开始安装 tauri-cli v1.6...
  cargo install tauri-cli --version "1.6.6" --locked
  if errorlevel 1 (
    echo [VeinMind] 安装失败，请检查网络后重试。
    pause
    exit /b 1
  )
)

echo [VeinMind] 编译检查中...
call cargo check
if errorlevel 1 (
  echo [VeinMind] 编译检查失败。
  pause
  exit /b 1
)

echo [VeinMind] 启动 VeinMind 开发模式...
call cargo tauri dev
pause
