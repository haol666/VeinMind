//! 端到端集成测试
//!
//! 这组测试不依赖 Tauri 运行时，直接驱动各层 trait 与函数，
//! 证明 VeinMind 的核心用例可运行：
//!   - 领域层：增删改查、层级、折叠、环防护
//!   - 命令模式：execute/undo/redo 的线性历史
//!   - 持久化：store/load 的序列化往返
//!   - 导出：Markdown/Outline/SVG/OPML 的产出
//!   - 布局：三种策略都能为每个节点产出坐标

use uuid::Uuid;

use crate::application::command::{
    add_child_command, add_edge_command, add_free_node_command, move_node_command,
    remove_node_command, set_free_pos_command, toggle_fold_command, update_notes_command,
    update_style_command, update_text_command, update_title_command, CommandInvoker,
};
use crate::application::layout::{
    CompactLayout, LayoutStrategy, LogicLayout, RadialLayout, TreeLayout,
};
use crate::domain::{MindMap, Node};
use crate::infrastructure::exporter;
use crate::infrastructure::persistence::{JsonMindMapRepository, MindMapRepository};

// ---------- 辅助 ----------

fn build_sample_map() -> MindMap {
    let mut map = MindMap::new("集成测试".into());
    let root = Node::new(Uuid::new_v4(), "根节点".into(), None);
    let root_id = root.id;
    map.add_node(root);
    map.set_root(root_id);
    // 加两个子节点
    let c1 = Node::new(Uuid::new_v4(), "子1".into(), Some(root_id));
    let c2 = Node::new(Uuid::new_v4(), "子2".into(), Some(root_id));
    map.add_node(c1);
    map.add_node(c2);
    map
}

// =================================================================
//  领域层
// =================================================================

#[test]
fn domain_add_and_children_order() {
    let map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let kids = map.children(root_id);
    assert_eq!(kids.len(), 2);
    // order 升序
    assert!(kids[0].order <= kids[1].order);
}

#[test]
fn domain_descendants_excludes_self() {
    let map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let desc = map.descendants(root_id);
    assert_eq!(desc.len(), 2);
    assert!(!desc.contains(&root_id));
}

#[test]
fn domain_remove_subtree_cleans_orphans() {
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();
    // 给子1 加个孙节点
    let kid = map.children(root_id)[0].clone();
    let kid_id = kid.id;
    let grand = Node::new(Uuid::new_v4(), "孙".into(), Some(kid_id));
    map.add_node(grand);
    // 删子1 → 孙也应消失
    map.remove_subtree(kid_id);
    assert_eq!(map.nodes.len(), 2); // 根 + 子2
    assert!(map.nodes.iter().all(|n| n.text != "孙"));
}

#[test]
fn domain_move_subtree_prevents_cycle() {
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let kid = map.children(root_id)[0].clone();
    // 试图把根移到子下 → 应被拒绝（不变更）
    map.move_subtree(root_id, kid.id);
    assert_eq!(map.get_node(root_id).unwrap().parent_id, None);
}

#[test]
fn domain_fold_toggles() {
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let was = map.get_node(root_id).unwrap().folded;
    map.toggle_fold(root_id);
    assert_ne!(was, map.get_node(root_id).unwrap().folded);
}

// =================================================================
//  命令模式：execute / undo / redo
// =================================================================

#[test]
fn command_undo_add_child() {
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let before = map.nodes.len();

    let mut inv = CommandInvoker::new(100);
    inv.execute(add_child_command(root_id, "新子".into()), &mut map);
    assert_eq!(map.nodes.len(), before + 1);

    assert!(inv.undo(&mut map));
    assert_eq!(map.nodes.len(), before);

    assert!(inv.redo(&mut map));
    assert_eq!(map.nodes.len(), before + 1);
}

#[test]
fn command_undo_add_free_node() {
    let mut map = build_sample_map();
    let before = map.nodes.len();

    let mut inv = CommandInvoker::new(100);
    inv.execute(add_free_node_command("漂浮主题".into()), &mut map);
    assert_eq!(map.nodes.len(), before + 1);
    // 游离节点：parent_id 为 None 且非根
    let free = map.free_nodes();
    assert_eq!(free.len(), 1);
    assert_eq!(free[0].text, "漂浮主题");

    assert!(inv.undo(&mut map));
    assert_eq!(map.nodes.len(), before);
    assert_eq!(map.free_nodes().len(), 0);
}

#[test]
fn command_add_remove_edge() {
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let kids: Vec<uuid::Uuid> = map.children(root_id).iter().map(|n| n.id).collect();
    assert!(kids.len() >= 2);
    let (a, b) = (kids[0], kids[1]);

    let mut inv = CommandInvoker::new(100);
    inv.execute(add_edge_command(a, b), &mut map);
    assert_eq!(map.edges.len(), 1, "add_edge should create one edge");
    let edge_id = map.edges[0].id;

    // 重复添加同一对端点 → 去重，不产生第二条
    inv.execute(add_edge_command(a, b), &mut map);
    assert_eq!(map.edges.len(), 1, "duplicate edge should be skipped");

    // 撤销两次：第一次撤销"重复添加"（无操作命令），第二次撤销真正创建边的命令
    assert!(inv.undo(&mut map));
    assert_eq!(map.edges.len(), 1, "first undo removes the no-op duplicate command");
    assert!(inv.undo(&mut map));
    assert_eq!(map.edges.len(), 0, "second undo removes the created edge");
    assert!(
        map.edges.iter().all(|e| e.id != edge_id),
        "removed edge should be the one created by the command"
    );
}

#[test]
fn domain_edge_removed_with_nodes() {
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let kid = map.children(root_id)[0].clone();
    let kid_id = kid.id;
    map.add_edge(root_id, kid_id);
    assert_eq!(map.edges.len(), 1);
    // 删除节点 → 相关边一并清除
    map.remove_subtree(kid_id);
    assert_eq!(map.edges.len(), 0, "edges touching removed nodes should be cleaned");
}

#[test]
fn command_undo_update_text() {
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();

    let mut inv = CommandInvoker::new(100);
    inv.execute(update_text_command(root_id, "改后".into()), &mut map);
    assert_eq!(map.get_node(root_id).unwrap().text, "改后");

    inv.undo(&mut map);
    assert_eq!(map.get_node(root_id).unwrap().text, "根节点");

    inv.redo(&mut map);
    assert_eq!(map.get_node(root_id).unwrap().text, "改后");
}

#[test]
fn command_undo_update_notes() {
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();

    let mut inv = CommandInvoker::new(100);
    inv.execute(update_notes_command(root_id, "第一条备注".into()), &mut map);
    assert_eq!(map.get_node(root_id).unwrap().notes, "第一条备注");

    inv.execute(update_notes_command(root_id, "第二条备注".into()), &mut map);
    assert_eq!(map.get_node(root_id).unwrap().notes, "第二条备注");

    inv.undo(&mut map);
    assert_eq!(map.get_node(root_id).unwrap().notes, "第一条备注");

    inv.undo(&mut map);
    assert_eq!(map.get_node(root_id).unwrap().notes, "");

    inv.redo(&mut map);
    assert_eq!(map.get_node(root_id).unwrap().notes, "第一条备注");
}

#[test]
fn command_undo_remove_with_subtree() {
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let kid = map.children(root_id)[0].clone();
    let before = map.nodes.len();

    let mut inv = CommandInvoker::new(100);
    inv.execute(remove_node_command(kid.id), &mut map);
    assert_eq!(map.nodes.len(), before - 1);

    inv.undo(&mut map);
    assert_eq!(map.nodes.len(), before);
    assert_eq!(map.get_node(kid.id).unwrap().text, "子1");

    inv.redo(&mut map);
    assert_eq!(map.nodes.len(), before - 1);
}

#[test]
fn command_new_command_clears_redo() {
    // 经典线性历史：执行 A → undo A → 执行 B → redo 应不可用
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let mut inv = CommandInvoker::new(100);

    inv.execute(add_child_command(root_id, "A".into()), &mut map);
    inv.undo(&mut map);
    inv.execute(add_child_command(root_id, "B".into()), &mut map);
    assert!(!inv.can_redo(), "redo should be cleared after new command");
}

#[test]
fn command_undo_move_node() {
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let kid = map.children(root_id)[0].clone();
    let other = map.children(root_id)[1].clone();

    let mut inv = CommandInvoker::new(100);
    inv.execute(move_node_command(kid.id, other.id), &mut map);
    assert_eq!(map.get_node(kid.id).unwrap().parent_id, Some(other.id));

    inv.undo(&mut map);
    assert_eq!(map.get_node(kid.id).unwrap().parent_id, Some(root_id));
}

#[test]
fn command_undo_toggle_fold() {
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let was = map.get_node(root_id).unwrap().folded;

    let mut inv = CommandInvoker::new(100);
    inv.execute(toggle_fold_command(root_id), &mut map);
    assert_ne!(was, map.get_node(root_id).unwrap().folded);

    inv.undo(&mut map);
    assert_eq!(was, map.get_node(root_id).unwrap().folded);
}

#[test]
fn style_icon_merge_and_undo() {
    use crate::domain::NodeStyle;
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();

    // 设置图标样式（⭐）→ merge_style 生效
    let style = NodeStyle {
        icon: Some("⭐".into()),
        ..Default::default()
    };
    let mut inv = CommandInvoker::new(100);
    inv.execute(update_style_command(root_id, style), &mut map);
    assert_eq!(map.get_node(root_id).unwrap().style.icon.as_deref(), Some("⭐"));

    // 撤销 → 图标还原为 None
    inv.undo(&mut map);
    assert_eq!(map.get_node(root_id).unwrap().style.icon, None);

    // 序列化保留 icon 字段
    let style2 = NodeStyle {
        icon: Some("🔥".into()),
        ..Default::default()
    };
    let mut inv2 = CommandInvoker::new(100);
    inv2.execute(update_style_command(root_id, style2), &mut map);
    let json = serde_json::to_string(&map).unwrap();
    assert!(json.contains("🔥"), "serialized map should keep the icon");
}

#[test]
fn style_clear_via_empty_string() {
    use crate::domain::NodeStyle;
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let mut inv = CommandInvoker::new(100);

    // 1) 填充颜色：设置 → 空字符串清除 → None
    inv.execute(
        update_style_command(
            root_id,
            NodeStyle { bg_color: Some("#FFE5E5".into()), ..Default::default() },
        ),
        &mut map,
    );
    assert_eq!(map.get_node(root_id).unwrap().style.bg_color.as_deref(), Some("#FFE5E5"));
    inv.execute(
        update_style_command(
            root_id,
            NodeStyle { bg_color: Some(String::new()), ..Default::default() },
        ),
        &mut map,
    );
    assert_eq!(
        map.get_node(root_id).unwrap().style.bg_color, None,
        "空字符串应清除填充颜色（取消填充色有效）"
    );

    // 2) 图标：设置 → 空字符串清除 → None
    inv.execute(
        update_style_command(
            root_id,
            NodeStyle { icon: Some("⭐".into()), ..Default::default() },
        ),
        &mut map,
    );
    assert_eq!(map.get_node(root_id).unwrap().style.icon.as_deref(), Some("⭐"));
    inv.execute(
        update_style_command(
            root_id,
            NodeStyle { icon: Some(String::new()), ..Default::default() },
        ),
        &mut map,
    );
    assert_eq!(
        map.get_node(root_id).unwrap().style.icon, None,
        "空字符串应清除图标（取消图标标记有效）"
    );

    // 3) 文字颜色：设置 → 空字符串清除 → None
    inv.execute(
        update_style_command(
            root_id,
            NodeStyle { fg_color: Some("#C42B2B".into()), ..Default::default() },
        ),
        &mut map,
    );
    assert_eq!(map.get_node(root_id).unwrap().style.fg_color.as_deref(), Some("#C42B2B"));
    inv.execute(
        update_style_command(
            root_id,
            NodeStyle { fg_color: Some(String::new()), ..Default::default() },
        ),
        &mut map,
    );
    assert_eq!(
        map.get_node(root_id).unwrap().style.fg_color, None,
        "空字符串应清除文字颜色"
    );
}

#[test]
fn style_font_size_merge() {
    use crate::domain::NodeStyle;
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let mut inv = CommandInvoker::new(100);

    // 字号 20 → merge 生效（背景/文本渲染随字号自适应）
    inv.execute(
        update_style_command(
            root_id,
            NodeStyle { font_size: Some(20), ..Default::default() },
        ),
        &mut map,
    );
    assert_eq!(map.get_node(root_id).unwrap().style.font_size, Some(20));

    // 撤销 → 还原 None（默认字号）
    inv.undo(&mut map);
    assert_eq!(map.get_node(root_id).unwrap().style.font_size, None);

    // 重做 → 再次 20
    inv.redo(&mut map);
    assert_eq!(map.get_node(root_id).unwrap().style.font_size, Some(20));
}

#[test]
fn style_fg_color_apply() {
    use crate::domain::NodeStyle;
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let mut inv = CommandInvoker::new(100);

    // 设置文字颜色（字体颜色"添加"生效）
    inv.execute(
        update_style_command(
            root_id,
            NodeStyle { fg_color: Some("#C42B2B".into()), ..Default::default() },
        ),
        &mut map,
    );
    assert_eq!(map.get_node(root_id).unwrap().style.fg_color.as_deref(), Some("#C42B2B"));

    // 序列化保留 fg_color 字段（属性面板/文件持久化可见）
    let json = serde_json::to_string(&map).unwrap();
    assert!(json.contains("#C42B2B"), "serialized map should keep fg_color");

    // 撤销 → None（默认色）；重做 → 恢复
    inv.undo(&mut map);
    assert_eq!(map.get_node(root_id).unwrap().style.fg_color, None);
    inv.redo(&mut map);
    assert_eq!(map.get_node(root_id).unwrap().style.fg_color.as_deref(), Some("#C42B2B"));
}

#[test]
fn command_undo_update_title() {
    let mut map = build_sample_map();
    let original = map.title.clone();

    let mut inv = CommandInvoker::new(100);
    inv.execute(update_title_command("新标题".into()), &mut map);
    assert_eq!(map.title, "新标题");

    inv.undo(&mut map);
    assert_eq!(map.title, original);

    inv.redo(&mut map);
    assert_eq!(map.title, "新标题");
}

// =================================================================
//  自由坐标（拖拽吸附终点持久化，根治"回到中心"缺陷）
// =================================================================

#[test]
fn command_undo_set_free_pos() {
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let kid = map.children(root_id)[0].clone();
    let kid_id = kid.id;

    let mut inv = CommandInvoker::new(100);
    inv.execute(set_free_pos_command(kid_id, 1234.0, 5678.0), &mut map);
    assert_eq!(map.get_node(kid_id).unwrap().free_pos(), Some((1234.0, 5678.0)));

    inv.undo(&mut map);
    assert_eq!(map.get_node(kid_id).unwrap().free_pos(), None);

    inv.redo(&mut map);
    assert_eq!(map.get_node(kid_id).unwrap().free_pos(), Some((1234.0, 5678.0)));
}

#[test]
fn layout_free_pos_priority() {
    use crate::application::layout::LayoutEngine;
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let kid = map.children(root_id)[0].clone();
    let kid_id = kid.id;

    // 设置自由坐标（模拟拖拽吸附终点）
    let mut inv = CommandInvoker::new(100);
    inv.execute(set_free_pos_command(kid_id, 999.0, 888.0), &mut map);

    // 布局引擎重算后，自由坐标优先于算法布局
    let engine = LayoutEngine::default();
    let layout = engine.compute(&map);
    let pos = layout.get(&kid_id).unwrap();
    assert_eq!(pos.x, 999.0, "free_pos should override algorithmic layout");
    assert_eq!(pos.y, 888.0);
}

#[test]
fn move_node_clears_free_pos() {
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let kids = map.children(root_id);
    let (a, b) = (kids[0].clone(), kids[1].clone());
    let a_id = a.id;

    // 先给 a 设置自由坐标，再移动到 b 下 → free_pos 应被清除
    let mut inv = CommandInvoker::new(100);
    inv.execute(set_free_pos_command(a_id, 111.0, 222.0), &mut map);
    inv.execute(move_node_command(a_id, b.id), &mut map);
    assert_eq!(map.get_node(a_id).unwrap().free_pos(), None, "move should clear free_pos");
}

#[test]
fn command_capacity_drops_oldest() {
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();
    let mut inv = CommandInvoker::new(2); // 仅保留 2 条
    for i in 0..3 {
        inv.execute(update_text_command(root_id, format!("v{i}")), &mut map);
    }
    // 最多 undo 2 次（容量 2），第 3 条已被丢弃
    let mut undone = 0;
    while inv.undo(&mut map) {
        undone += 1;
    }
    assert!(undone <= 2, "capacity should cap undo depth");
}

// =================================================================
//  持久化：store / load 往返
// =================================================================

#[test]
fn persistence_roundtrip() {
    let map = build_sample_map();
    let repo = JsonMindMapRepository::with_initial(map.clone());

    let loaded = repo.load().unwrap();
    assert_eq!(loaded.title, map.title);
    assert_eq!(loaded.nodes.len(), map.nodes.len());
    assert_eq!(loaded.root_id, map.root_id);
}

#[test]
fn persistence_json_serialize() {
    let map = build_sample_map();
    let repo = JsonMindMapRepository::with_initial(map);
    let json = repo.to_json().unwrap();
    assert!(json.contains("集成测试"));
    assert!(json.contains("根节点"));
}

#[test]
fn persistence_from_json_replaces_document() {
    let map1 = build_sample_map();
    let repo = JsonMindMapRepository::with_initial(map1);

    // 构造第二张图并序列化
    let mut map2 = MindMap::new("另一张".into());
    let r = Node::new(Uuid::new_v4(), "R2".into(), None);
    let r_id = r.id;
    map2.add_node(r);
    map2.set_root(r_id);
    let json2 = serde_json::to_string(&map2).unwrap();

    let loaded = repo.from_json(&json2).unwrap();
    assert_eq!(loaded.title, "另一张");
}

#[test]
fn persistence_flush_writes_atomic_file() {
    use std::io::Read;

    let mut map = build_sample_map();
    map.set_title("落盘测试".into());
    let repo = JsonMindMapRepository::with_initial(map.clone());
    let tmp = std::env::temp_dir().join(format!("veinmind_flush_{}.json", Uuid::new_v4()));
    repo.set_path(tmp.clone());
    // 自动保存去抖窗口内 store 不落盘，flush 强制写盘
    repo.store(map).unwrap();
    repo.flush().unwrap();

    let mut content = String::new();
    std::fs::File::open(&tmp).unwrap().read_to_string(&mut content).unwrap();
    assert!(content.contains("落盘测试"), "flush 后文件应包含最新内容");
    assert!(!tmp.with_extension("json.tmp").exists(), "原子写不应残留临时文件");
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn persistence_store_error_surfaces_as_warning() {
    let map = build_sample_map();
    let repo = JsonMindMapRepository::with_initial(map.clone());
    // 指向一个必然失败的路径（不存在目录），store 应返回错误而非吞掉
    let bad = std::path::PathBuf::from("/nonexistent_dir_veinmind/out.json");
    repo.set_path(bad);
    repo.store(map).unwrap(); // 首次可能被去抖合并，不落盘不算错误
    let result = repo.flush();
    assert!(result.is_err(), "不可写路径的 flush 应报错");
}

// =================================================================
//  导出：Markdown / Outline / SVG / OPML
// =================================================================

#[test]
fn export_markdown_contains_root() {
    let map = build_sample_map();
    let md = exporter::export(&map, "markdown");
    assert!(md.contains("# 根节点"), "markdown should have root heading");
    assert!(md.contains("- 子1"), "markdown should list children");
}

#[test]
fn export_outline_indents() {
    let map = build_sample_map();
    let outline = exporter::export(&map, "outline");
    assert!(outline.contains("根节点"));
    assert!(outline.contains("子1"));
}

#[test]
fn export_svg_uses_layout_coordinates() {
    let map = build_sample_map();
    let svg = exporter::export(&map, "svg");
    // 真实矢量视图应含 <rect>、<text>、<path>
    assert!(svg.contains("<rect"), "SVG should render node rectangles");
    assert!(svg.contains("<text"), "SVG should render node text");
    assert!(svg.contains("<path"), "SVG should render connection paths");
    assert!(svg.contains("根节点"), "SVG should contain root text");
}

#[test]
fn export_opml_roundtrip() {
    let map = build_sample_map();
    let opml = exporter::export(&map, "opml");
    assert!(opml.contains("<opml version=\"1.0\">"));
    assert!(opml.contains("根节点"));

    // 反向导入
    let imported = exporter::import_opml(&opml).unwrap();
    assert_eq!(imported.nodes.len(), map.nodes.len());
    assert!(imported.root_id.is_some());
}

#[test]
fn export_freemind_xml() {
    let map = build_sample_map();
    let mm = exporter::export(&map, "freemind");
    // FreeMind .mm 结构：<map version="1.0.1"> + <node TEXT="...">
    assert!(mm.contains("<map version=\"1.0.1\">"));
    assert!(mm.contains("TEXT=\"根节点\""));
    assert!(mm.contains("TEXT=\"子1\""));
    assert!(mm.contains("TEXT=\"子2\""));
    // "mm" 别名同样生效
    let mm2 = exporter::export(&map, "mm");
    assert_eq!(mm, mm2);
}

#[test]
fn import_freemind_roundtrip() {
    let map = build_sample_map();
    // 导出 → 再导入，节点数与层级应还原
    let mm = exporter::export(&map, "freemind");
    let imported = exporter::import_freemind(&mm).unwrap();
    assert_eq!(imported.nodes.len(), map.nodes.len(), "roundtrip should restore node count");
    assert!(imported.root_id.is_some());
    // 根节点文本应保留
    let root = imported.get_node(imported.root_id.unwrap()).unwrap();
    assert_eq!(root.text, "根节点");
}

// =================================================================
//  布局：三种策略都为每个节点产出坐标
// =================================================================

#[test]
fn layout_tree_covers_all_nodes() {
    let map = build_sample_map();
    let layout = TreeLayout::default().compute(&map);
    assert_eq!(layout.len(), map.nodes.len(), "tree layout should cover all nodes");
    // 根在 x=0
    let root_pos = layout.get(&map.root_id.unwrap()).unwrap();
    assert!((root_pos.x - 0.0).abs() < 0.001, "root x should be 0");
}

#[test]
fn layout_radial_covers_all_nodes() {
    let map = build_sample_map();
    let layout = RadialLayout::default().compute(&map);
    assert_eq!(layout.len(), map.nodes.len(), "radial layout should cover all nodes");
    // 根在原点
    let root_pos = layout.get(&map.root_id.unwrap()).unwrap();
    assert!(root_pos.x.abs() < 0.001 && root_pos.y.abs() < 0.001);
}

#[test]
fn layout_compact_covers_all_nodes() {
    let map = build_sample_map();
    let layout = CompactLayout::default().compute(&map);
    assert_eq!(layout.len(), map.nodes.len(), "compact layout should cover all nodes");
}

#[test]
fn layout_logic_covers_all_nodes() {
    let map = build_sample_map();
    let layout = LogicLayout::default().compute(&map);
    assert_eq!(layout.len(), map.nodes.len(), "logic layout should cover all nodes");
    // 根节点居中于 x=0，一级子节点左右交替分布
    let root_pos = layout.get(&map.root_id.unwrap()).copied().unwrap();
    assert!(root_pos.x.abs() < 0.001, "logic layout root x should be 0");
    let kids = map.children(map.root_id.unwrap());
    assert_eq!(kids.len(), 2);
    let k0 = layout.get(&kids[0].id).unwrap();
    let k1 = layout.get(&kids[1].id).unwrap();
    assert!(
        (k0.x < 0.0 && k1.x > 0.0) || (k0.x > 0.0 && k1.x < 0.0),
        "logic layout should place first-level children on opposite sides"
    );
}

#[test]
fn layout_empty_map_returns_empty() {
    let map = MindMap::new("空".into());
    let layout = TreeLayout::default().compute(&map);
    assert!(layout.is_empty());
}

// =================================================================
//  跨层端到端：命令 → 持久化 → 导出
// =================================================================

#[test]
fn e2e_command_then_persist_then_export() {
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();

    // 1. 命令：加子节点
    let mut inv = CommandInvoker::new(100);
    inv.execute(add_child_command(root_id, "端到端子".into()), &mut map);

    // 2. 持久化
    let repo = JsonMindMapRepository::with_initial(map.clone());
    let loaded = repo.load().unwrap();
    assert_eq!(loaded.nodes.len(), map.nodes.len());

    // 3. 导出
    let md = exporter::export(&loaded, "markdown");
    assert!(md.contains("端到端子"), "exported markdown should contain new child");
}

#[test]
fn e2e_undo_restores_state_and_exports() {
    let mut map = build_sample_map();
    let root_id = map.root_id.unwrap();

    let mut inv = CommandInvoker::new(100);
    inv.execute(update_text_command(root_id, "临时改名".into()), &mut map);

    // 撤销后导出，应看到原始根名
    inv.undo(&mut map);
    let md = exporter::export(&map, "markdown");
    assert!(md.contains("根节点"), "after undo, export should show original root text");
    assert!(!md.contains("临时改名"));
}
