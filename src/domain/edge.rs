//! 边（关系线）实体。
//!
//! 除了隐式的父子关系，思维导图常需要显式的“跨节点关系线”，
//! 故单独抽象 `Edge`。当前 MVP 仅保留结构，供未来扩展。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 节点之间的关系线。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: Uuid,
    pub source: Uuid,
    pub target: Uuid,
    pub label: Option<String>,
    pub curve: EdgeCurve,
}

/// 关系线曲线类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeCurve {
    #[default]
    Straight,
    Bezier,
    Arc,
}

impl Edge {
    pub fn new(source: Uuid, target: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            source,
            target,
            label: None,
            curve: EdgeCurve::default(),
        }
    }
}
