//! 应用层 (Application Layer)
//!
//! 负责用例编排，调度领域层与基础设施层。包含三个核心设计模式：
//!   - [`command`] —— **Command 模式**：每个操作封装为可撤销对象。
//!   - [`event_bus`] —— **Observer 模式**：节点变更事件广播。
//!   - [`layout`] —— **Strategy 模式**：布局算法可插拔。

pub mod command;
pub mod event_bus;
pub mod layout;
