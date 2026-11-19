//! 思维导图聚合根。
//!
//! `MindMap` 是领域层的 **聚合根 (Aggregate Root)**：所有对节点/边的
//! 变更都必须通过 `MindMap` 的方法发起，从而保证树形结构的不变量
//! (invariants)：
//!   1. 有且仅有一个根节点 (`root_id`)。
//!   2. 除根节点外，每个节点都有且仅有一个 `parent_id`，且父节点存在。
//!   3. 不存在环。

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Edge, Node};

/// 思维导图文档（聚合根）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindMap {
    pub id: Uuid,
    pub title: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub root_id: Option<Uuid>,
    /// `id → nodes 下标` 索引，加速 `get_node`/`get_node_mut`（O(1) 定位，替代线性扫描）。
    /// 不参与序列化；反序列化/删除节点后需调用 [`MindMap::rebuild_index`] 重建。
    #[serde(skip)]
    node_index: HashMap<Uuid, usize>,
}

impl MindMap {
    /// 构造一张空白思维导图。
    pub fn new(title: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            title,
            nodes: Vec::new(),
            edges: Vec::new(),
            root_id: None,
            node_index: HashMap::new(),
        }
    }

    /// 根据当前 `nodes` 重建 id 索引（删除节点或反序列化后调用）。
    pub fn rebuild_index(&mut self) {
        self.node_index = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id, i))
            .collect();
    }

    /// 更新文档标题。
    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    // ---------- 节点查询 ----------

    /// 按 id 查节点（O(1) 索引定位）。
    pub fn get_node(&self, id: Uuid) -> Option<&Node> {
        self.node_index.get(&id).and_then(|&i| self.nodes.get(i))
    }

    /// 按 id 查可变节点（O(1) 索引定位）。
    pub fn get_node_mut(&mut self, id: Uuid) -> Option<&mut Node> {
        let idx = self.node_index.get(&id).copied();
        idx.and_then(|i| self.nodes.get_mut(i))
    }

    /// 返回某节点的所有直接子节点（按 `order` 升序）。
    pub fn children(&self, parent_id: Uuid) -> Vec<&Node> {
        let mut kids: Vec<&Node> = self
            .nodes
            .iter()
            .filter(|n| n.parent_id == Some(parent_id))
            .collect();
        kids.sort_by_key(|n| n.order);
        kids
    }

    /// 一次性构建"父 id → 子 id 列表（按 order 升序）"的邻接表。
    /// 布局算法与后代遍历可复用此表，避免对每个节点反复全表扫描（O(n²) → O(n)）。
    pub fn children_map(&self) -> HashMap<Uuid, Vec<Uuid>> {
        let mut map: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        for n in &self.nodes {
            if let Some(pid) = n.parent_id {
                map.entry(pid).or_default().push(n.id);
            }
        }
        for ids in map.values_mut() {
            ids.sort_by_key(|id| self.get_node(*id).map(|n| n.order).unwrap_or(0));
        }
        map
    }

    /// 返回某节点的所有后代 id（不含自身）。基于邻接表遍历，整体 O(n)。
    pub fn descendants(&self, id: Uuid) -> Vec<Uuid> {
        let children = self.children_map();
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            if let Some(kids) = children.get(&cur) {
                for kid in kids {
                    out.push(*kid);
                    stack.push(*kid);
                }
            }
        }
        out
    }

    /// 返回所有游离节点（parent_id 为 None 且非根节点的漂浮主题）。
    pub fn free_nodes(&self) -> Vec<&Node> {
        self.nodes
            .iter()
            .filter(|n| n.parent_id.is_none() && Some(n.id) != self.root_id)
            .collect()
    }

    // ---------- 节点变更（仅定义原语，不处理 undo） ----------

    /// 新增节点。返回新节点 id。`order` 自动取父节点下最大序号 + 1。
    pub fn add_node(&mut self, mut node: Node) -> Uuid {
        if let Some(pid) = node.parent_id {
            let next_order = self
                .children(pid)
                .last()
                .map(|n| n.order + 1)
                .unwrap_or(0);
            node.order = next_order;
        }
        let id = node.id;
        self.node_index.insert(id, self.nodes.len());
        self.nodes.push(node);
        id
    }

    /// 新增游离节点（漂浮主题）：parent_id 为 None，不挂在树上。
    /// 布局时由 [`crate::application::layout::LayoutEngine`] 放置到主树外侧。
    pub fn add_free_node(&mut self, text: String) -> Uuid {
        let mut node = Node::new(Uuid::new_v4(), text, None);
        // 游离节点按加入顺序分配 order，供布局纵向排布
        node.order = self.free_nodes().len() as u32;
        let id = node.id;
        self.node_index.insert(id, self.nodes.len());
        self.nodes.push(node);
        id
    }

    /// 删除节点及其全部后代。若删除的是根节点则清空整图。
    pub fn remove_subtree(&mut self, id: Uuid) {
        let to_remove: HashSet<Uuid> = std::iter::once(id)
            .chain(self.descendants(id))
            .collect();
        self.nodes.retain(|n| !to_remove.contains(&n.id));
        self.edges
            .retain(|e| !to_remove.contains(&e.source) && !to_remove.contains(&e.target));
        self.rebuild_index();

        if self.root_id == Some(id) {
            self.root_id = None;
        }
    }

    /// 设置根节点（首次创建时调用）。
    pub fn set_root(&mut self, id: Uuid) {
        self.root_id = Some(id);
    }

    /// 移动子树：将 `node_id` 的父节点改为 `new_parent_id`，并重排兄弟序号。
    pub fn move_subtree(&mut self, node_id: Uuid, new_parent_id: Uuid) {
        if node_id == new_parent_id { return; }
        // 防止把父节点移到子节点下（会成环）
        if self.descendants(node_id).contains(&new_parent_id) {
            return;
        }
        // 先算新父节点下的兄弟数量，再修改，避免可变/不可变借用冲突。
        let new_order = self.children(new_parent_id).len() as u32;
        if let Some(node) = self.get_node_mut(node_id) {
            node.parent_id = Some(new_parent_id);
            node.order = new_order;
        }
    }

    /// 新增关系线（跨节点自定义连接）。返回边 id。
    /// 若两点间已存在关系线则跳过（保持唯一）。
    pub fn add_edge(&mut self, source: Uuid, target: Uuid) -> Option<Uuid> {
        if source == target {
            return None;
        }
        let dup = self.edges.iter().any(|e| {
            (e.source == source && e.target == target)
                || (e.source == target && e.target == source)
        });
        if dup {
            return None;
        }
        let edge = Edge::new(source, target);
        let id = edge.id;
        self.edges.push(edge);
        Some(id)
    }

    /// 删除关系线。
    pub fn remove_edge(&mut self, edge_id: Uuid) -> bool {
        let before = self.edges.len();
        self.edges.retain(|e| e.id != edge_id);
        self.edges.len() < before
    }

    /// 切换某节点折叠状态。
    pub fn toggle_fold(&mut self, id: Uuid) {
        if let Some(node) = self.get_node_mut(id) {
            node.toggle_fold();
        }
    }

    /// 更新节点文本。
    pub fn update_text(&mut self, id: Uuid, text: String) {
        if let Some(node) = self.get_node_mut(id) {
            node.set_text(text);
        }
    }

    /// 更新节点备注。
    pub fn update_notes(&mut self, id: Uuid, notes: String) {
        if let Some(node) = self.get_node_mut(id) {
            node.notes = notes;
        }
    }

    /// 合并节点样式。
    pub fn merge_style(&mut self, id: Uuid, style: &super::NodeStyle) {
        if let Some(node) = self.get_node_mut(id) {
            node.merge_style(style);
        }
    }
}
