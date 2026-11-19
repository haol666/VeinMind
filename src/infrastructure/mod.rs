//! 基础设施层 (Infrastructure Layer)
//!
//! 技术细节落地：持久化、文件 I/O、导出器。依赖领域层，不依赖应用层。

pub mod exporter;
pub mod persistence;
