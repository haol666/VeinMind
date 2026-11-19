//! 观察者模式 (Observer Pattern)
//!
//! `EventBus` 充当事件中心，业务层在节点变更后 `publish` 事件，
//! 订阅者（日志、自动保存、UI 刷新等）通过 `subscribe` 注册回调。
//! 解耦“事件源”与“事件处理”，符合 **开闭原则**。

use std::sync::Mutex;

use uuid::Uuid;

/// 领域事件类型。
#[derive(Debug, Clone)]
pub enum DomainEvent {
    /// 文档被替换/新建/加载。
    DocumentReset { title: String },
    /// 文档标题更新。
    DocumentTitleUpdated { title: String },
    /// 节点新增。
    NodeAdded { node_id: Uuid, parent_id: Option<Uuid> },
    /// 节点删除（含子树）。
    NodeRemoved { node_id: Uuid },
    /// 节点文本更新。
    NodeTextUpdated { node_id: Uuid, old: String, new: String },
    /// 节点样式更新（背景/前景/字号/图标等）。
    NodeStyleUpdated { node_id: Uuid },
    /// 节点备注更新。
    NodeNotesUpdated { node_id: Uuid },
    /// 节点折叠/展开。
    NodeFoldToggled { node_id: Uuid, folded: bool },
    /// 子树移动。
    NodeMoved { node_id: Uuid, new_parent_id: Uuid },
    /// 节点自由坐标持久化（拖拽吸附终点）。
    NodeFreePosSet { node_id: Uuid },
    /// 关系线新增。
    EdgeAdded { edge_id: Uuid, source: Uuid, target: Uuid },
    /// 关系线删除。
    EdgeRemoved { edge_id: Uuid },
    /// 撤销/重做触发。
    HistoryChanged { can_undo: bool, can_redo: bool },
}

/// 事件处理器签名：`fn(&DomainEvent)`。
type Handler = Box<dyn Fn(&DomainEvent) + Send + Sync>;

/// 事件总线。
pub struct EventBus {
    handlers: Mutex<Vec<Handler>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            handlers: Mutex::new(Vec::new()),
        }
    }

    /// 订阅事件。返回的回调会在 `publish` 时被同步调用。
    pub fn subscribe<F>(&self, handler: F)
    where
        F: Fn(&DomainEvent) + Send + Sync + 'static,
    {
        self.handlers.lock().unwrap().push(Box::new(handler));
    }

    /// 发布事件，依次调用所有订阅者。
    pub fn publish(&self, event: &DomainEvent) {
        if let Ok(handlers) = self.handlers.lock() {
            for h in handlers.iter() {
                h(event);
            }
        }
    }

    /// 清空所有订阅者（例如文档切换时）。
    pub fn clear(&self) {
        self.handlers.lock().unwrap().clear();
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
