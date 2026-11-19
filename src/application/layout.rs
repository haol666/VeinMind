//! 布局服务 (Strategy Pattern)
//!
//! 把“如何给节点排坐标”抽象为一族可互换的布局策略，每族实现
//! [`LayoutStrategy`] trait。[`LayoutEngine`] 作为上下文 (Context)，
//! 持有当前策略并对外提供 `compute` 入口。
//!
//! 三种策略：
//!   - [`TreeLayout`]      —— 经典横向树，根在左、子向右展开。
//!   - [`RadialLayout`]    —— 放射状，根居中、子按角度向外。
//!   - [`CompactLayout`]   —— 紧凑纵向树，适合大纲浏览。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{MindMap, Node};

/// 单个节点的布局坐标。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// 节点 id → 坐标 的映射，即一次布局计算的结果。
pub type Layout = HashMap<Uuid, Point>;

/// 估算节点的显示尺寸 `(宽, 高)`，供布局算法避免长文本/多行节点重叠。
/// 规则与前端 `measureNode` 对齐：宽 = 最长行字符数 × 每字符宽 + 内边距；
/// 高 = 行数 × 行高 + 内边距。
fn estimate_node_size(node: &Node) -> (f32, f32) {
    let lines = node.line_count().max(1) as f32;
    let max_chars = node.max_line_chars().max(1) as f32;
    let w = max_chars * 8.0 + 24.0;
    let h = lines * 18.0 + 12.0;
    (w, h)
}

/// 父子节点水平间距：父宽一半 + 最小间距 + 子宽一半，保证父子列不重叠。
fn child_x(parent_x: f32, parent_w: f32, child_w: f32, min_gap: f32) -> f32 {
    parent_x + parent_w / 2.0 + min_gap + child_w / 2.0
}

/// 布局策略公共接口。
pub trait LayoutStrategy: Send + Sync {
    fn name(&self) -> &'static str;
    /// 计算并返回整张思维导图的坐标布局。
    fn compute(&self, map: &MindMap) -> Layout;
}

// =================================================================
//  Tree Layout (横向树)
// =================================================================

/// 横向树布局。根节点 x=0，每深一级 x += H_GAP；
/// 同层兄弟 y 按子树高度居中堆叠。
pub struct TreeLayout {
    pub h_gap: f32,
    pub v_gap: f32,
    pub node_height: f32,
}

impl Default for TreeLayout {
    fn default() -> Self {
        Self {
            h_gap: 180.0,
            v_gap: 16.0,
            node_height: 28.0,
        }
    }
}

impl LayoutStrategy for TreeLayout {
    fn name(&self) -> &'static str {
        "tree"
    }
    fn compute(&self, map: &MindMap) -> Layout {
        let mut layout: Layout = HashMap::new();
        let root_id = match map.root_id {
            Some(id) => id,
            None => return layout,
        };
        // 一次性构建父子邻接表，避免每个节点反复全表扫描（O(n²) → O(n)）
        let children = map.children_map();
        // 先递归算每个子树的高度，再分配 y。
        let _subtree_height = assign_tree(map, root_id, 0.0, 0.0, &children, &mut layout, self);
        layout
    }
}

/// 递归：返回当前子树所占的垂直高度。
fn assign_tree(
    map: &MindMap,
    id: Uuid,
    x: f32,
    y_top: f32,
    children: &HashMap<Uuid, Vec<Uuid>>,
    layout: &mut Layout,
    cfg: &TreeLayout,
) -> f32 {
    let kids: &[Uuid] = children.get(&id).map(|v| v.as_slice()).unwrap_or(&[]);
    let node = match map.get_node(id) {
        Some(n) => n,
        None => return 0.0,
    };
    // 按文本实际尺寸估算节点高度，避免多行/长文本节点重叠
    let (node_w, node_h) = estimate_node_size(node);
    let h = node_h.max(cfg.node_height);

    // 折叠或叶子：占一个节点高度
    if node.folded || kids.is_empty() {
        layout.insert(id, Point { x, y: y_top + h / 2.0 });
        return h;
    }

    let mut cursor = y_top;
    let mut total = 0.0;
    for child in kids {
        // 子节点 x 随文本宽度推进，避免父子列重叠
        let child_w = map
            .get_node(*child)
            .map(|n| estimate_node_size(n).0)
            .unwrap_or(cfg.node_height);
        let child_x = child_x(x, node_w, child_w, cfg.h_gap);
        let ch = assign_tree(map, *child, child_x, cursor, children, layout, cfg);
        cursor += ch + cfg.v_gap;
        total += ch + cfg.v_gap;
    }
    total -= cfg.v_gap; // 末尾多余 gap

    // 父节点 y 居中于其子节点群
    let first = layout.get(&kids[0]).copied();
    let last = layout.get(kids.last().unwrap()).copied();
    let py = match (first, last) {
        (Some(a), Some(b)) => (a.y + b.y) / 2.0,
        _ => y_top + h / 2.0,
    };
    layout.insert(id, Point { x, y: py });
    total.max(h)
}

// =================================================================
//  Radial Layout (放射状)
// =================================================================

/// 放射状布局。根居中 (0,0)，每一层按角度均分。
pub struct RadialLayout {
    pub radius_step: f32,
}

impl Default for RadialLayout {
    fn default() -> Self {
        Self { radius_step: 160.0 }
    }
}

impl LayoutStrategy for RadialLayout {
    fn name(&self) -> &'static str {
        "radial"
    }
    fn compute(&self, map: &MindMap) -> Layout {
        let mut layout: Layout = HashMap::new();
        let root_id = match map.root_id {
            Some(id) => id,
            None => return layout,
        };
        let children = map.children_map();
        layout.insert(root_id, Point { x: 0.0, y: 0.0 });
        assign_radial(map, root_id, 0.0, std::f32::consts::TAU, 1, &children, &mut layout, self);
        layout
    }
}

fn assign_radial(
    map: &MindMap,
    id: Uuid,
    angle_start: f32,
    angle_end: f32,
    depth: usize,
    children: &HashMap<Uuid, Vec<Uuid>>,
    layout: &mut Layout,
    cfg: &RadialLayout,
) {
    let node = match map.get_node(id) {
        Some(n) => n,
        None => return,
    };
    let kids: &[Uuid] = children.get(&id).map(|v| v.as_slice()).unwrap_or(&[]);
    if node.folded || kids.is_empty() {
        return;
    }
    let span = angle_end - angle_start;
    let step = span / kids.len() as f32;
    let r = cfg.radius_step * depth as f32;
    for (i, child) in kids.iter().enumerate() {
        let a = angle_start + step * (i as f32 + 0.5);
        let x = r * a.cos();
        let y = r * a.sin();
        layout.insert(*child, Point { x, y });
        // 子节点角度范围
        let cs = angle_start + step * i as f32;
        let ce = cs + step;
        assign_radial(map, *child, cs, ce, depth + 1, children, layout, cfg);
    }
}

// =================================================================
//  Compact Layout (紧凑纵向)
// =================================================================

/// 紧凑纵向布局。根在顶，子节点向下展开，适合大纲。
pub struct CompactLayout {
    pub h_gap: f32,
    pub v_gap: f32,
    pub node_height: f32,
}

impl Default for CompactLayout {
    fn default() -> Self {
        Self {
            h_gap: 24.0,
            v_gap: 36.0,
            node_height: 24.0,
        }
    }
}

impl LayoutStrategy for CompactLayout {
    fn name(&self) -> &'static str {
        "compact"
    }
    fn compute(&self, map: &MindMap) -> Layout {
        let mut layout: Layout = HashMap::new();
        let root_id = match map.root_id {
            Some(id) => id,
            None => return layout,
        };
        let children = map.children_map();
        assign_compact(map, root_id, 0.0, 0, &children, &mut layout, self);
        layout
    }
}

fn assign_compact(
    map: &MindMap,
    id: Uuid,
    y: f32,
    depth: usize,
    children: &HashMap<Uuid, Vec<Uuid>>,
    layout: &mut Layout,
    cfg: &CompactLayout,
) -> f32 {
    let x = depth as f32 * cfg.h_gap;
    let node = match map.get_node(id) {
        Some(n) => n,
        None => return 0.0,
    };
    // 按文本实际尺寸估算节点高度，多行节点不重叠
    let (_, node_h) = estimate_node_size(node);
    let h = node_h.max(cfg.node_height);
    layout.insert(id, Point { x, y: y + h / 2.0 });
    if node.folded {
        return h;
    }
    let kids: &[Uuid] = children.get(&id).map(|v| v.as_slice()).unwrap_or(&[]);
    let mut cursor = y + h + cfg.v_gap;
    let mut total = 0.0;
    for child in kids {
        let ch = assign_compact(map, *child, cursor, depth + 1, children, layout, cfg);
        cursor += ch + cfg.v_gap;
        total += ch + cfg.v_gap;
    }
    // 子树总高 = 自身高度 + 全部子节点高度（含间距）
    total + h
}

// =================================================================
//  Logic Layout (左右双向树，XMind 逻辑图风格)
// =================================================================

/// 逻辑图布局：根节点居中，一级子节点按 `order` 奇偶交替分布在左右两侧，
/// 深层子树沿各自方向继续向外展开（左子树向左、右子树向右）。
/// 对应 XMind 的"逻辑图"结构。
pub struct LogicLayout {
    pub h_gap: f32,
    pub v_gap: f32,
    pub node_height: f32,
}

impl Default for LogicLayout {
    fn default() -> Self {
        Self {
            h_gap: 180.0,
            v_gap: 16.0,
            node_height: 28.0,
        }
    }
}

impl LayoutStrategy for LogicLayout {
    fn name(&self) -> &'static str {
        "logic"
    }
    fn compute(&self, map: &MindMap) -> Layout {
        let mut layout: Layout = HashMap::new();
        let root_id = match map.root_id {
            Some(id) => id,
            None => return layout,
        };
        let children = map.children_map();
        // 先递归布局所有一级子树（偶数序在左、奇数序在右），再居中根节点。
        let kids: &[Uuid] = children.get(&root_id).map(|v| v.as_slice()).unwrap_or(&[]);
        if kids.is_empty() {
            layout.insert(root_id, Point { x: 0.0, y: 0.0 });
            return layout;
        }
        let mut cursor_y = 0.0;
        let mut first_y = 0.0;
        let mut last_y = 0.0;
        for (i, kid) in kids.iter().enumerate() {
            let dir: f32 = if i % 2 == 0 { -1.0 } else { 1.0 };
            let h = assign_logic(map, *kid, dir * self.h_gap, cursor_y, dir, &children, &mut layout, self);
            let py = cursor_y + h / 2.0;
            if i == 0 {
                first_y = py;
            }
            last_y = py;
            cursor_y += h + self.v_gap;
        }
        let root_y = (first_y + last_y) / 2.0;
        layout.insert(root_id, Point { x: 0.0, y: root_y });
        layout
    }
}

/// 递归：方向 `dir`（-1 左 / +1 右），返回子树高度。
fn assign_logic(
    map: &MindMap,
    id: Uuid,
    x: f32,
    y_top: f32,
    dir: f32,
    children: &HashMap<Uuid, Vec<Uuid>>,
    layout: &mut Layout,
    cfg: &LogicLayout,
) -> f32 {
    let kids: &[Uuid] = children.get(&id).map(|v| v.as_slice()).unwrap_or(&[]);
    let node = match map.get_node(id) {
        Some(n) => n,
        None => return 0.0,
    };
    // 按文本实际尺寸估算节点高度，多行节点不重叠
    let (node_w, node_h) = estimate_node_size(node);
    let h = node_h.max(cfg.node_height);

    if node.folded || kids.is_empty() {
        layout.insert(id, Point { x, y: y_top + h / 2.0 });
        return h;
    }

    let mut cursor = y_top;
    let mut total = 0.0;
    for child in kids {
        // 子节点 x 随文本宽度推进（左右方向各自向外），避免父子列重叠
        let child_w = map
            .get_node(*child)
            .map(|n| estimate_node_size(n).0)
            .unwrap_or(cfg.node_height);
        let child_x = x + dir * (node_w / 2.0 + cfg.h_gap + child_w / 2.0);
        let ch = assign_logic(map, *child, child_x, cursor, dir, children, layout, cfg);
        cursor += ch + cfg.v_gap;
        total += ch + cfg.v_gap;
    }
    total -= cfg.v_gap;

    let first = layout.get(&kids[0]).copied();
    let last = layout.get(kids.last().unwrap()).copied();
    let py = match (first, last) {
        (Some(a), Some(b)) => (a.y + b.y) / 2.0,
        _ => y_top + h / 2.0,
    };
    layout.insert(id, Point { x, y: py });
    total.max(h)
}

// =================================================================
//  Layout Engine (Strategy Context)
// =================================================================

/// 布局引擎：上下文对象，持有当前策略。
///
/// 切换策略只需 `engine.set_strategy("radial")`，调用方不关心具体
/// 实现——典型的 **策略模式** 用法。
pub struct LayoutEngine {
    strategy: Box<dyn LayoutStrategy>,
}

impl LayoutEngine {
    pub fn new(strategy: Box<dyn LayoutStrategy>) -> Self {
        Self { strategy }
    }

    /// 根据名称切换策略。未知名称默认回退到 TreeLayout。
    pub fn set_strategy(&mut self, name: &str) {
        self.strategy = make_strategy(name);
    }

    /// 当前策略名称。
    pub fn strategy_name(&self) -> &'static str {
        self.strategy.name()
    }

    /// 计算布局。
    ///
    /// 先委托当前策略计算主树坐标，再为游离节点（漂浮主题）分配坐标：
    /// 放置在主树包围盒右侧、纵向排布。这样多根/漂浮节点不参与树布局，
    /// 但始终有坐标可渲染。
    ///
    /// 最后应用**自由坐标**：用户拖拽吸附后持久化的 `free_pos` 优先于
    /// 算法布局，保证拖拽终点不会因任何后续布局重算被拉回（"回到中心"
    /// 缺陷的根治）。
    pub fn compute(&self, map: &MindMap) -> Layout {
        let mut layout = self.strategy.compute(map);

        let free: Vec<_> = map.free_nodes();
        if !free.is_empty() {
            // 主树包围盒右缘（无主树时用固定偏移）
            let mut max_x = f32::MIN;
            for p in layout.values() {
                if p.x > max_x {
                    max_x = p.x;
                }
            }
            let anchor_x = if max_x.is_finite() {
                max_x + 220.0
            } else {
                300.0
            };
            // 按加入顺序（order）纵向排布
            let mut y = 0.0_f32;
            for node in free {
                layout.insert(node.id, Point { x: anchor_x, y });
                y += 80.0;
            }
        }

        // 自由坐标覆盖：用户拖拽吸附的终点优先于算法布局
        for node in &map.nodes {
            if let Some((x, y)) = node.free_pos() {
                layout.insert(node.id, Point { x, y });
            }
        }

        layout
    }
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new(make_strategy("tree"))
    }
}

/// 策略工厂：名称 → 具体策略实例。
fn make_strategy(name: &str) -> Box<dyn LayoutStrategy> {
    match name.to_lowercase().as_str() {
        "radial" => Box::new(RadialLayout::default()),
        "compact" => Box::new(CompactLayout::default()),
        "logic" => Box::new(LogicLayout::default()),
        _ => Box::new(TreeLayout::default()),
    }
}
