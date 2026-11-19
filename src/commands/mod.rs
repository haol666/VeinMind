//! Tauri 命令层 (Presentation Layer · IPC Bridge)
//!
//! 这里定义的所有 `#[tauri::command]` 函数，会被 `generate_handler!`
//! 注册到 Tauri 运行时，前端通过 `invoke('name', args)` 调用。
//!
//! 设计要点：
//! - 命令仅做 **参数解包 → 调用应用层 → 返回 DTO**，不含业务逻辑。
//! - 共享状态通过 `tauri::State<'_, AppState>` 注入（DI 思想）。

pub mod dto;
pub mod mindmap;
