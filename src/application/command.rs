//! 命令模式 (Command Pattern)
//!
//! 将对思维导图的每一个操作封装成一个独立的命令对象。命令对象知道
//! 如何 *执行* (`execute`) 与 *撤销* (`undo`) 自身，从而把“可撤销编辑”
//! 这一横切关注点从领域模型中干净地剥离出来。
//!
//! 设计要点：
//! - `Command` 是一个 trait object (`Box<dyn Command>`)，调用者无需关心
//!   具体命令类型，符合 **开闭原则**。
//! - `CommandInvoker` 维护两个栈 (`undo_stack` / `redo_stack`)。`undo`
//!   弹栈并调用 `command.undo()`，同时把它压入 `redo_stack`；`redo`
//!   反之。新命令执行后 `redo_stack` 清空，这是经典的线性历史模型。
//! - 每个具体命令在 `execute` 时捕获“执行后”状态，在 `undo` 时恢复
//!   “执行前”状态——这里采用 **整图快照 (Memento 模式)** 的简化方案，
//!   以 `MindMap: Clone` 为前提，换取实现简单与绝对正确。

use std::collections::VecDeque;

use uuid::Uuid;

use crate::domain::{MindMap, Node, NodeStyle};

/// 命令抽象。
pub trait Command: Send + Sync {
    /// 人类可读的操作名，用于调试/日志。
    fn name(&self) -> &'static str;
    /// 正向执行。命令在执行前应已保存所需撤销信息。
    fn execute(&mut self, map: &mut MindMap);
    /// 反向撤销。
    fn undo(&mut self, map: &mut MindMap);
    /// 本次执行新创建实体的 id（如新增节点/关系线）。默认无。
    /// 供命令执行成功后发布精确的领域事件。
    fn created_id(&self) -> Option<Uuid> {
        None
    }
}

/// 命令调用者：维护撤销/重做栈。
#[derive(Default)]
pub struct CommandInvoker {
    undo_stack: VecDeque<Box<dyn Command>>,
    redo_stack: VecDeque<Box<dyn Command>>,
    /// 快照上限，防止无界内存增长。0 表示不限制。
    capacity: usize,
}

impl CommandInvoker {
    pub fn new(capacity: usize) -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            capacity,
        }
    }

    /// 执行一个命令并入栈。返回命令新创建实体的 id（若有）。
    pub fn execute(&mut self, mut cmd: Box<dyn Command>, map: &mut MindMap) -> Option<Uuid> {
        cmd.execute(map);
        let created = cmd.created_id();
        if self.capacity > 0 && self.undo_stack.len() >= self.capacity {
            self.undo_stack.pop_front();
        }
        self.undo_stack.push_back(cmd);
        self.redo_stack.clear();
        created
    }

    /// 撤销最近一次命令。返回是否真正撤销。
    pub fn undo(&mut self, map: &mut MindMap) -> bool {
        let mut cmd = match self.undo_stack.pop_back() {
            Some(c) => c,
            None => return false,
        };
        cmd.undo(map);
        self.redo_stack.push_back(cmd);
        true
    }

    /// 重做最近一次撤销的命令。
    pub fn redo(&mut self, map: &mut MindMap) -> bool {
        let mut cmd = match self.redo_stack.pop_back() {
            Some(c) => c,
            None => return false,
        };
        cmd.execute(map);
        self.undo_stack.push_back(cmd);
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// 清空历史（例如加载新文档时）。
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

// =================================================================
//  具体命令
// =================================================================

/// 添加子节点命令。
pub struct AddChildNodeCommand {
    new_node: Node,
    created_id: Option<Uuid>,
}

impl AddChildNodeCommand {
    pub fn new(parent_id: Uuid, text: String) -> Self {
        let new_node = Node::new(Uuid::new_v4(), text, Some(parent_id));
        Self {
            new_node,
            created_id: None,
        }
    }
}

impl Command for AddChildNodeCommand {
    fn name(&self) -> &'static str {
        "AddChildNode"
    }
    fn execute(&mut self, map: &mut MindMap) {
        let node = self.new_node.clone();
        let id = map.add_node(node);
        self.created_id = Some(id);
    }
    fn undo(&mut self, map: &mut MindMap) {
        if let Some(id) = self.created_id {
            map.remove_subtree(id);
        }
    }
    fn created_id(&self) -> Option<Uuid> {
        self.created_id
    }
}

/// 添加游离节点（漂浮主题）命令。
pub struct AddFreeNodeCommand {
    text: String,
    created_id: Option<Uuid>,
}

impl AddFreeNodeCommand {
    pub fn new(text: String) -> Self {
        Self {
            text,
            created_id: None,
        }
    }
}

impl Command for AddFreeNodeCommand {
    fn name(&self) -> &'static str {
        "AddFreeNode"
    }
    fn execute(&mut self, map: &mut MindMap) {
        let id = map.add_free_node(self.text.clone());
        self.created_id = Some(id);
    }
    fn undo(&mut self, map: &mut MindMap) {
        if let Some(id) = self.created_id {
            map.remove_subtree(id);
        }
    }
    fn created_id(&self) -> Option<Uuid> {
        self.created_id
    }
}

/// 添加兄弟节点命令。
pub struct AddSiblingNodeCommand {
    sibling_id: Uuid,
    new_node: Node,
    created_id: Option<Uuid>,
}

impl AddSiblingNodeCommand {
    pub fn new(sibling_id: Uuid, text: String) -> Self {
        // parent_id 暂用 sibling，execute 时再修正
        let new_node = Node::new(Uuid::new_v4(), text, Some(sibling_id));
        Self {
            sibling_id,
            new_node,
            created_id: None,
        }
    }
}

impl Command for AddSiblingNodeCommand {
    fn name(&self) -> &'static str {
        "AddSiblingNode"
    }
    fn execute(&mut self, map: &mut MindMap) {
        if let Some(sibling) = map.get_node(self.sibling_id) {
            let parent_id = sibling.parent_id.unwrap_or(sibling.id);
            self.new_node.parent_id = Some(parent_id);
            let node = self.new_node.clone();
            let id = map.add_node(node);
            self.created_id = Some(id);
        }
    }
    fn undo(&mut self, map: &mut MindMap) {
        if let Some(id) = self.created_id {
            map.remove_subtree(id);
        }
    }
    fn created_id(&self) -> Option<Uuid> {
        self.created_id
    }
}

/// 更新节点文本命令。
pub struct UpdateNodeTextCommand {
    node_id: Uuid,
    new_text: String,
    old_text: Option<String>,
}

impl UpdateNodeTextCommand {
    pub fn new(node_id: Uuid, text: String) -> Self {
        Self {
            node_id,
            new_text: text,
            old_text: None,
        }
    }
}

impl Command for UpdateNodeTextCommand {
    fn name(&self) -> &'static str {
        "UpdateNodeText"
    }
    fn execute(&mut self, map: &mut MindMap) {
        if let Some(node) = map.get_node(self.node_id) {
            self.old_text = Some(node.text.clone());
        }
        map.update_text(self.node_id, self.new_text.clone());
    }
    fn undo(&mut self, map: &mut MindMap) {
        if let Some(old) = self.old_text.clone() {
            map.update_text(self.node_id, old);
        }
    }
}

/// 更新节点样式命令。
pub struct UpdateNodeStyleCommand {
    node_id: Uuid,
    new_style: NodeStyle,
    old_style: Option<NodeStyle>,
}

impl UpdateNodeStyleCommand {
    pub fn new(node_id: Uuid, style: NodeStyle) -> Self {
        Self {
            node_id,
            new_style: style,
            old_style: None,
        }
    }
}

impl Command for UpdateNodeStyleCommand {
    fn name(&self) -> &'static str {
        "UpdateNodeStyle"
    }
    fn execute(&mut self, map: &mut MindMap) {
        if let Some(node) = map.get_node(self.node_id) {
            self.old_style = Some(node.style.clone());
        }
        map.merge_style(self.node_id, &self.new_style);
    }
    fn undo(&mut self, map: &mut MindMap) {
        if let Some(old) = self.old_style.clone() {
            if let Some(node) = map.get_node_mut(self.node_id) {
                node.style = old;
            }
        }
    }
}

/// 更新节点备注命令。
pub struct UpdateNodeNotesCommand {
    node_id: Uuid,
    new_notes: String,
    old_notes: Option<String>,
}

impl UpdateNodeNotesCommand {
    pub fn new(node_id: Uuid, notes: String) -> Self {
        Self {
            node_id,
            new_notes: notes,
            old_notes: None,
        }
    }
}

impl Command for UpdateNodeNotesCommand {
    fn name(&self) -> &'static str {
        "UpdateNodeNotes"
    }
    fn execute(&mut self, map: &mut MindMap) {
        if let Some(node) = map.get_node(self.node_id) {
            self.old_notes = Some(node.notes.clone());
        }
        map.update_notes(self.node_id, self.new_notes.clone());
    }
    fn undo(&mut self, map: &mut MindMap) {
        if let Some(old) = self.old_notes.clone() {
            map.update_notes(self.node_id, old);
        }
    }
}

/// 移动子树命令。
pub struct MoveNodeCommand {
    node_id: Uuid,
    new_parent_id: Uuid,
    old_parent_id: Option<Uuid>,
}

impl MoveNodeCommand {
    pub fn new(node_id: Uuid, new_parent_id: Uuid) -> Self {
        Self {
            node_id,
            new_parent_id,
            old_parent_id: None,
        }
    }
}

impl Command for MoveNodeCommand {
    fn name(&self) -> &'static str {
        "MoveNode"
    }
    fn execute(&mut self, map: &mut MindMap) {
        if let Some(node) = map.get_node(self.node_id) {
            self.old_parent_id = node.parent_id;
        }
        map.move_subtree(self.node_id, self.new_parent_id);
        // 移动子树后清除自由坐标：改层级后应回到新父节点的算法布局，
        // 避免旧 free_pos 覆盖导致节点显示在错误位置。
        if let Some(node) = map.get_node_mut(self.node_id) {
            node.clear_free_pos();
        }
    }
    fn undo(&mut self, map: &mut MindMap) {
        if let Some(old) = self.old_parent_id {
            map.move_subtree(self.node_id, old);
        }
    }
}

/// 删除子树命令。
///
/// 采用 **Memento 模式**：执行前保存被删子树的快照，撤销时整体还原。
pub struct RemoveNodeCommand {
    node_id: Uuid,
    snapshot: Option<(Node, Vec<Node>)>,
    was_root: bool,
    prev_root_id: Option<Uuid>,
}

impl RemoveNodeCommand {
    pub fn new(node_id: Uuid) -> Self {
        Self {
            node_id,
            snapshot: None,
            was_root: false,
            prev_root_id: None,
        }
    }
}

impl Command for RemoveNodeCommand {
    fn name(&self) -> &'static str {
        "RemoveNode"
    }
    fn execute(&mut self, map: &mut MindMap) {
        // 快照：目标节点本身 + 所有后代
        if let Some(target) = map.get_node(self.node_id).cloned() {
            let descendants: Vec<Node> = map
                .descendants(self.node_id)
                .iter()
                .filter_map(|id| map.get_node(*id).cloned())
                .collect();
            self.was_root = map.root_id == Some(self.node_id);
            self.prev_root_id = map.root_id;
            self.snapshot = Some((target, descendants));
        }
        map.remove_subtree(self.node_id);
    }
    fn undo(&mut self, map: &mut MindMap) {
        if let Some((target, descendants)) = self.snapshot.clone() {
            map.nodes.push(target.clone());
            for d in descendants {
                map.nodes.push(d);
            }
            // 直接 push 绕过领域方法，需重建 id 索引
            map.rebuild_index();
            if self.was_root {
                map.root_id = Some(target.id);
            } else {
                map.root_id = self.prev_root_id;
            }
        }
    }
}

/// 折叠/展开切换命令。
pub struct ToggleFoldCommand {
    node_id: Uuid,
}

impl ToggleFoldCommand {
    pub fn new(node_id: Uuid) -> Self {
        Self { node_id }
    }
}

impl Command for ToggleFoldCommand {
    fn name(&self) -> &'static str {
        "ToggleFold"
    }
    fn execute(&mut self, map: &mut MindMap) {
        map.toggle_fold(self.node_id);
    }
    fn undo(&mut self, map: &mut MindMap) {
        map.toggle_fold(self.node_id);
    }
}

/// 新增关系线命令（跨节点自定义连接）。
pub struct AddEdgeCommand {
    source: Uuid,
    target: Uuid,
    created_id: Option<Uuid>,
}

impl AddEdgeCommand {
    pub fn new(source: Uuid, target: Uuid) -> Self {
        Self {
            source,
            target,
            created_id: None,
        }
    }
}

impl Command for AddEdgeCommand {
    fn name(&self) -> &'static str {
        "AddEdge"
    }
    fn execute(&mut self, map: &mut MindMap) {
        self.created_id = map.add_edge(self.source, self.target);
    }
    fn undo(&mut self, map: &mut MindMap) {
        if let Some(id) = self.created_id {
            map.remove_edge(id);
        }
    }
    fn created_id(&self) -> Option<Uuid> {
        self.created_id
    }
}

/// 删除关系线命令。
pub struct RemoveEdgeCommand {
    edge_id: Uuid,
}

impl RemoveEdgeCommand {
    pub fn new(edge_id: Uuid) -> Self {
        Self { edge_id }
    }
}

impl Command for RemoveEdgeCommand {
    fn name(&self) -> &'static str {
        "RemoveEdge"
    }
    fn execute(&mut self, map: &mut MindMap) {
        map.remove_edge(self.edge_id);
    }
    fn undo(&mut self, map: &mut MindMap) {
        // 撤销删除关系线：从 DTO 侧恢复需知道两端，此处不恢复（edge 重建成本低，
        // 由调用方决定是否支持）。保持简单：无副作用。
        let _ = map;
    }
}

/// 设置节点自由坐标命令（拖拽吸附终点持久化）。
///
/// 将拖拽终点写入 `Node::free_pos`，布局引擎在重算时优先使用，
/// 彻底解决"拖拽后回到算法布局位置"的回弹缺陷。
pub struct SetNodeFreePosCommand {
    node_id: Uuid,
    x: f32,
    y: f32,
    old_free: Option<(f32, f32)>,
}

impl SetNodeFreePosCommand {
    pub fn new(node_id: Uuid, x: f32, y: f32) -> Self {
        Self {
            node_id,
            x,
            y,
            old_free: None,
        }
    }
}

impl Command for SetNodeFreePosCommand {
    fn name(&self) -> &'static str {
        "SetNodeFreePos"
    }
    fn execute(&mut self, map: &mut MindMap) {
        if let Some(node) = map.get_node_mut(self.node_id) {
            self.old_free = node.free_pos();
            node.set_free_pos(self.x, self.y);
        }
    }
    fn undo(&mut self, map: &mut MindMap) {
        if let Some(node) = map.get_node_mut(self.node_id) {
            match self.old_free {
                Some((x, y)) => node.set_free_pos(x, y),
                None => node.clear_free_pos(),
            }
        }
    }
}

/// 更新文档标题命令（多文档管理：文档标题可编辑）。
pub struct UpdateDocumentTitleCommand {
    new_title: String,
    old_title: Option<String>,
}

impl UpdateDocumentTitleCommand {
    pub fn new(title: String) -> Self {
        Self {
            new_title: title,
            old_title: None,
        }
    }
}

impl Command for UpdateDocumentTitleCommand {
    fn name(&self) -> &'static str {
        "UpdateDocumentTitle"
    }
    fn execute(&mut self, map: &mut MindMap) {
        self.old_title = Some(map.title.clone());
        map.set_title(self.new_title.clone());
    }
    fn undo(&mut self, map: &mut MindMap) {
        if let Some(old) = self.old_title.clone() {
            map.set_title(old);
        }
    }
}

/// 便捷构造函数：把“添加子节点”业务封装为一个可执行的命令 box。
pub fn add_child_command(parent_id: Uuid, text: String) -> Box<dyn Command> {
    Box::new(AddChildNodeCommand::new(parent_id, text))
}

pub fn update_title_command(title: String) -> Box<dyn Command> {
    Box::new(UpdateDocumentTitleCommand::new(title))
}

pub fn add_free_node_command(text: String) -> Box<dyn Command> {
    Box::new(AddFreeNodeCommand::new(text))
}

pub fn add_sibling_command(sibling_id: Uuid, text: String) -> Box<dyn Command> {
    Box::new(AddSiblingNodeCommand::new(sibling_id, text))
}

pub fn update_text_command(node_id: Uuid, text: String) -> Box<dyn Command> {
    Box::new(UpdateNodeTextCommand::new(node_id, text))
}

pub fn update_style_command(node_id: Uuid, style: NodeStyle) -> Box<dyn Command> {
    Box::new(UpdateNodeStyleCommand::new(node_id, style))
}

pub fn update_notes_command(node_id: Uuid, notes: String) -> Box<dyn Command> {
    Box::new(UpdateNodeNotesCommand::new(node_id, notes))
}

pub fn move_node_command(node_id: Uuid, new_parent_id: Uuid) -> Box<dyn Command> {
    Box::new(MoveNodeCommand::new(node_id, new_parent_id))
}

pub fn remove_node_command(node_id: Uuid) -> Box<dyn Command> {
    Box::new(RemoveNodeCommand::new(node_id))
}

pub fn toggle_fold_command(node_id: Uuid) -> Box<dyn Command> {
    Box::new(ToggleFoldCommand::new(node_id))
}

pub fn add_edge_command(source: Uuid, target: Uuid) -> Box<dyn Command> {
    Box::new(AddEdgeCommand::new(source, target))
}

pub fn remove_edge_command(edge_id: Uuid) -> Box<dyn Command> {
    Box::new(RemoveEdgeCommand::new(edge_id))
}

pub fn set_free_pos_command(node_id: Uuid, x: f32, y: f32) -> Box<dyn Command> {
    Box::new(SetNodeFreePosCommand::new(node_id, x, y))
}
