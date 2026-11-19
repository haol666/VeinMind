//! 数据传输对象 (DTO)
//!
//! 这些结构是应用层与前端之间的契约。与领域模型解耦，便于
//! 独立演进序列化格式，同时避免把领域细节直接暴露给前端。
//!
//! 注意：当前 Tauri 命令直接使用基础类型 (Uuid/String) 作为参数，
//! 故 `*Params` 类型仅作契约文档保留，便于未来切换为强类型参数。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::application::layout::Point;
use crate::domain::MindMap;

/// 前端持有的"文档快照"：思维导图 + 布局坐标。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentDto {
    pub mind_map: MindMap,
    pub layout: HashMap<Uuid, Point>,
    pub can_undo: bool,
    pub can_redo: bool,
    /// 非致命警告（如自动保存失败），供前端 toast 提示。
    #[serde(default)]
    pub warning: Option<String>,
}

/// 新建文档请求参数（契约文档）。
#[derive(Debug, Clone, Deserialize)]
pub struct CreateDocumentParams {
    pub title: Option<String>,
}

/// 添加子节点参数（契约文档）。
#[derive(Debug, Clone, Deserialize)]
pub struct AddChildParams {
    pub parent_id: Uuid,
    pub text: String,
}

/// 添加兄弟节点参数（契约文档）。
#[derive(Debug, Clone, Deserialize)]
pub struct AddSiblingParams {
    pub sibling_id: Uuid,
    pub text: String,
}

/// 更新节点文本参数（契约文档）。
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateTextParams {
    pub node_id: Uuid,
    pub text: String,
}

/// 移动节点参数（契约文档）。
#[derive(Debug, Clone, Deserialize)]
pub struct MoveNodeParams {
    pub node_id: Uuid,
    pub new_parent_id: Uuid,
}

/// 单 ID 参数（删除、折叠等，契约文档）。
#[derive(Debug, Clone, Deserialize)]
pub struct NodeIdParams {
    pub node_id: Uuid,
}

/// 设置布局策略参数（契约文档）。
#[derive(Debug, Clone, Deserialize)]
pub struct SetStrategyParams {
    pub strategy: String,
}

/// 导出参数（契约文档）。
#[derive(Debug, Clone, Deserialize)]
pub struct ExportParams {
    pub format: String,
}
