//! 领域层 (Domain Layer)
//!
//! 纯业务模型，零外部依赖。所有实体均派生 `Serialize`/`Deserialize` 以便
//! 持久化，并实现 `Clone` 以支持 Command 模式的撤销快照。

pub mod edge;
pub mod mind_map;
pub mod node;
pub mod visitor;

pub use edge::Edge;
pub use mind_map::MindMap;
pub use node::{Node, NodeStyle};
pub use visitor::{NodeVisitor, TraversalKind, traverse};
