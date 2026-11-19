//! 访问者模式 (Visitor Pattern)
//!
//! 将“对节点树的操作”从“节点数据结构”中解耦。新增导出格式只需新增
//! 一个 `NodeVisitor` 实现，而无需修改 `Node`/`MindMap`。

use uuid::Uuid;

/// 节点访问者 trait。
///
/// `visit_root` / `visit_node` / `visit_end` 在遍历过程中被调用，
/// 实现者负责收集信息或输出结果。
pub trait NodeVisitor {
    /// 访问根节点时调用。
    fn visit_root(&mut self, text: &str);
    /// 访问普通节点时调用。`depth` 为相对根的层级（根=0）。
    fn visit_node(&mut self, text: &str, depth: usize);
    /// 访问某节点子树结束时调用（用于闭合标签等）。
    fn visit_end(&mut self, depth: usize) {
        let _ = depth;
    }
    /// 返回最终产物（例如 Markdown 字符串）。
    fn result(&self) -> String;
}

/// 遍历方式。
#[derive(Debug, Clone, Copy, Default)]
pub enum TraversalKind {
    /// 深度优先（前序）。
    #[default]
    DepthFirst,
    /// 广度优先。
    BreadthFirst,
}

/// 在 `MindMap` 上执行访问者遍历的入口。
///
/// 这是一个 **外部迭代器 (external iterator)** 风格的遍历函数，
/// 避免把遍历逻辑塞进 `MindMap` 自身。
pub fn traverse<V: NodeVisitor>(
    map: &super::MindMap,
    kind: TraversalKind,
    visitor: &mut V,
) {
    let root_id = match map.root_id {
        Some(id) => id,
        None => return,
    };

    match kind {
        TraversalKind::DepthFirst => dfs(map, root_id, 0, visitor),
        TraversalKind::BreadthFirst => bfs(map, root_id, visitor),
    }
}

fn dfs<V: NodeVisitor>(map: &super::MindMap, id: Uuid, depth: usize, v: &mut V) {
    let node = match map.get_node(id) {
        Some(n) => n,
        None => return,
    };
    if depth == 0 {
        v.visit_root(&node.text);
    } else {
        v.visit_node(&node.text, depth);
    }

    if !node.folded {
        for child in map.children(id) {
            dfs(map, child.id, depth + 1, v);
        }
    }
    v.visit_end(depth);
}

fn bfs<V: NodeVisitor>(map: &super::MindMap, root: Uuid, v: &mut V) {
    use std::collections::VecDeque;
    let mut queue: VecDeque<(Uuid, usize)> = VecDeque::new();
    queue.push_back((root, 0));

    while let Some((id, depth)) = queue.pop_front() {
        let node = match map.get_node(id) {
            Some(n) => n,
            None => continue,
        };
        if depth == 0 {
            v.visit_root(&node.text);
        } else {
            v.visit_node(&node.text, depth);
        }
        if !node.folded {
            for child in map.children(id) {
                queue.push_back((child.id, depth + 1));
            }
        }
    }
}
