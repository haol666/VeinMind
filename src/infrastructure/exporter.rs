//! 导出器 (Visitor 模式实现)
//!
//! 每个导出器实现 `NodeVisitor`，把思维导图节点树转换为不同格式的文本。
//! 新增导出格式只需新增一个实现，无需改动 `Node`/`MindMap`。

use std::fmt::Write;

use crate::domain::{MindMap, Node, NodeVisitor, TraversalKind, traverse};

/// Markdown 导出器。
///
/// 根节点输出为 `#` 一级标题，其后每层深度增加一个 `#`，
/// 子节点用 `-` 列表呈现，折叠节点后追加 ` ▸`。
pub struct MarkdownExporter {
    out: String,
}

impl MarkdownExporter {
    pub fn new() -> Self {
        Self { out: String::new() }
    }
}

impl Default for MarkdownExporter {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeVisitor for MarkdownExporter {
    fn visit_root(&mut self, text: &str) {
        let _ = writeln!(self.out, "# {}\n", text);
    }
    fn visit_node(&mut self, text: &str, depth: usize) {
        let indent = "  ".repeat(depth.saturating_sub(1));
        let _ = writeln!(self.out, "{}- {}", indent, text);
    }
    fn visit_end(&mut self, _depth: usize) {
        // Markdown 无需闭合
    }
    fn result(&self) -> String {
        self.out.clone()
    }
}

/// OPML 大纲导出器（简化版，纯文本缩进）。
pub struct OutlineExporter {
    out: String,
}

impl OutlineExporter {
    pub fn new() -> Self {
        Self { out: String::new() }
    }
}

impl Default for OutlineExporter {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeVisitor for OutlineExporter {
    fn visit_root(&mut self, text: &str) {
        let _ = writeln!(self.out, "{}", text);
    }
    fn visit_node(&mut self, text: &str, depth: usize) {
        let indent = "  ".repeat(depth);
        let _ = writeln!(self.out, "{}{}", indent, text);
    }
    fn visit_end(&mut self, _depth: usize) {}
    fn result(&self) -> String {
        self.out.clone()
    }
}

/// SVG 导出器：基于 [`crate::application::layout`] 计算的坐标，
/// 把节点树渲染为真实矢量视图（含连线、节点矩形、文本）。
///
/// 这是对早期"大纲式文本堆叠"实现的替换——现在 SVG 导出与
/// Canvas 前端共用同一套布局坐标，产出所见即所得的矢量图。
///
/// 用 LayoutEngine 坐标渲染 SVG。
///
/// 入口签名独立于 `NodeVisitor`，因为需要访问布局坐标，
/// 而访问者只回调文本——这里改用直接遍历 `MindMap`。
pub fn export_svg(map: &MindMap, layout: &crate::application::layout::Layout) -> String {
    if map.nodes.is_empty() {
        return empty_svg();
    }

    // 计算包围盒以确定 viewBox
    let (min_x, min_y, max_x, max_y) = bbox(map, layout);
    let pad = 40.0_f32;
    let w = (max_x - min_x).max(200.0) + pad * 2.0;
    let h = (max_y - min_y).max(160.0) + pad * 2.0;
    let ox = min_x - pad;
    let oy = min_y - pad;

    let mut out = String::new();
    out.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    out.push('\n');
    out.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="{ox} {oy} {w} {h}">"##,
        w = w as u32, h = h as u32, ox = ox as i32, oy = oy as i32
    ));
    out.push('\n');
    out.push_str(r##"<rect x="{ox}" y="{oy}" width="{w}" height="{h}" fill="#ffffff"/>"##);
    out.push('\n');

    // 先画连线（父→子）
    for node in &map.nodes {
        let parent = node.parent_id.and_then(|pid| map.get_node(pid));
        let parent = match parent {
            Some(p) => p,
            None => continue,
        };
        if parent.folded {
            continue;
        }
        let p = match layout.get(&parent.id) {
            Some(pt) => *pt,
            None => continue,
        };
        let c = match layout.get(&node.id) {
            Some(pt) => *pt,
            None => continue,
        };
        let mid_x = (p.x + c.x) / 2.0;
        let _ = write!(
            out,
            r##"<path d="M {px} {py} C {mx} {py}, {mx} {cy}, {cx} {cy}" stroke="#6c7086" stroke-width="1.5" fill="none"/>"##,
            px = p.x, py = p.y, mx = mid_x, cy = c.y, cx = c.x
        );
        out.push('\n');
    }

    // 再画节点
    for node in &map.nodes {
        let pos = match layout.get(&node.id) {
            Some(pt) => *pt,
            None => continue,
        };
        let is_root = node.id == map.root_id.unwrap_or(node.id);
        let plain = node.plain_text();
        let label = escape_xml(&plain);
        // 节点宽度：粗略按字符数估算
        let char_w = if is_root { 11.0 } else { 8.0 };
        let nw = (plain.chars().count() as f32 * char_w + 24.0).max(60.0);
        let nh = if is_root { 36.0 } else { 28.0 };
        let fill = if is_root { "#89b4fa" } else { "#cdd6f4" };
        let text_color = if is_root { "#1e1e2e" } else { "#1e1e2e" };
        let font_size = if is_root { 16 } else { 14 };
        let rx = pos.x - nw / 2.0;
        let ry = pos.y - nh / 2.0;

        let _ = write!(
            out,
            r##"<rect x="{rx}" y="{ry}" width="{nw}" height="{nh}" rx="6" fill="{fill}" stroke="#45475a"/>"##,
            rx = rx as i32, ry = ry as i32, nw = nw as u32, nh = nh as u32, fill = fill
        );
        out.push('\n');

        let _ = write!(
            out,
            r##"<text x="{tx}" y="{ty}" text-anchor="middle" dominant-baseline="middle" font-family="sans-serif" font-size="{fs}" fill="{tc}">{label}</text>"##,
            tx = pos.x as i32, ty = pos.y as i32, fs = font_size, tc = text_color, label = label
        );
        out.push('\n');

        // 折叠标记
        if node.folded {
            let _ = write!(
                out,
                r##"<text x="{}" y="{}" font-size="12" fill="#a6adc8">▸</text>"##,
                (pos.x + nw / 2.0 + 4.0) as i32,
                (pos.y + 4.0) as i32
            );
            out.push('\n');
        }
    }

    out.push_str("</svg>");
    out
}

/// 计算布局的包围盒 (min_x, min_y, max_x, max_y)。
fn bbox(
    map: &MindMap,
    layout: &crate::application::layout::Layout,
) -> (f32, f32, f32, f32) {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for node in &map.nodes {
        if let Some(p) = layout.get(&node.id) {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
    }
    if min_x.is_finite() {
        (min_x, min_y, max_x, max_y)
    } else {
        (0.0, 0.0, 200.0, 160.0)
    }
}

fn empty_svg() -> String {
    r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="200" height="160" viewBox="0 0 200 160">
<rect width="100%" height="100%" fill="#ffffff"/>
</svg>"##
        .to_string()
}

/// 统一导出入口。
///
/// 根据 `format` 字符串选择具体访问者，对思维导图执行 DFS 遍历。
/// `format` 取值：`"markdown"` / `"outline"` / `"svg"` / `"opml"` / `"freemind"`。
pub fn export(map: &MindMap, format: &str) -> String {
    match format.to_lowercase().as_str() {
        "outline" => {
            let mut v = OutlineExporter::new();
            traverse(map, TraversalKind::DepthFirst, &mut v);
            v.result()
        }
        "svg" => {
            // 用默认 TreeLayout 计算坐标，再渲染真实矢量视图
            use crate::application::layout::{LayoutStrategy, TreeLayout};
            let strategy = TreeLayout::default();
            let layout = strategy.compute(map);
            export_svg(map, &layout)
        }
        "opml" => export_opml(map),
        "freemind" | "mm" => export_freemind(map),
        _ => {
            // 默认 markdown
            let mut v = MarkdownExporter::new();
            traverse(map, TraversalKind::DepthFirst, &mut v);
            v.result()
        }
    }
}

// =================================================================
//  OPML 导入/导出 (Outline Processor Markup Language)
// =================================================================
//
// OPML 是大纲类工具（Workflowy、幕布、Feedly、RSS 阅读器）的事实交换
// 标准。VeinMind 支持 OPML 双向转换，保证与生态互通。
//
// OPML 1.0 结构：
//   <opml version="1.0">
//     <head><title>...</title></head>
//     <body>
//       <outline text="根节点">
//         <outline text="子节点1"/>
//         <outline text="子节点2">
//           <outline text="孙节点"/>
//         </outline>
//       </outline>
//     </body>
//   </opml>
//
// VeinMind 的 `Node.text` 存储原始 Markdown 子集字符串，OPML 导出时
// 直接放入 `text` 属性，导入时反向解析。

/// 导出为 OPML 1.0 XML。
pub fn export_opml(map: &MindMap) -> String {
    let root_id = match map.root_id {
        Some(id) => id,
        None => return empty_opml("Untitled"),
    };
    let root = match map.get_node(root_id) {
        Some(n) => n,
        None => return empty_opml("Untitled"),
    };

    let mut out = String::new();
    out.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    out.push('\n');
    out.push_str(r##"<opml version="1.0">"##);
    out.push('\n');
    out.push_str("<head>");
    out.push('\n');
    out.push_str(&format!(
        "<title>{}</title>",
        escape_xml(&map.title)
    ));
    out.push('\n');
    out.push_str("</head>");
    out.push('\n');
    out.push_str("<body>");
    out.push('\n');
    // 递归写 outline
    write_outline(&mut out, root, map, 1);
    out.push_str("</body>");
    out.push('\n');
    out.push_str("</opml>");
    out
}

fn write_outline(out: &mut String, node: &Node, map: &MindMap, depth: usize) {
    let indent = "  ".repeat(depth);
    let children: Vec<&Node> = map.children(node.id);
    let text = escape_xml(&node.text);
    if children.is_empty() {
        let _ = writeln!(out, "{}<outline text=\"{}\" _folded=\"{}\"/>", indent, text, node.folded);
    } else {
        let _ = writeln!(
            out,
            "{}<outline text=\"{}\" _folded=\"{}\">",
            indent, text, node.folded
        );
        for child in &children {
            write_outline(out, child, map, depth + 1);
        }
        let _ = writeln!(out, "{}</outline>", indent);
    }
}

fn empty_opml(title: &str) -> String {
    format!(
        r##"<?xml version="1.0" encoding="UTF-8"?>
<opml version="1.0">
<head><title>{}</title></head>
<body/>
</opml>"##,
        escape_xml(title)
    )
}

/// OPML XML 转义。
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// =================================================================
//  FreeMind .mm 导出 (XML)
// =================================================================
//
// FreeMind 用 `.mm` (XML) 存储导图，是 XMind / MindManager / 幕布 /
// ProcessOn / 百度脑图 等工具共同支持的事实交换格式。VeinMind 导出
// FreeMind 格式即可与上述生态互通。
//
// 结构：
//   <map version="1.0.1">
//     <node TEXT="根节点">
//       <node TEXT="子节点"/>
//     </node>
//   </map>

/// 导出为 FreeMind `.mm` XML。
pub fn export_freemind(map: &MindMap) -> String {
    let root_id = match map.root_id {
        Some(id) => id,
        None => return empty_freemind(),
    };
    let root = match map.get_node(root_id) {
        Some(n) => n,
        None => return empty_freemind(),
    };

    let mut out = String::new();
    out.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    out.push('\n');
    out.push_str(r##"<map version="1.0.1">"##);
    out.push('\n');
    write_freemind_node(&mut out, root, map, 1, true);
    out.push_str("</map>");
    out
}

fn write_freemind_node(
    out: &mut String,
    node: &Node,
    map: &MindMap,
    depth: usize,
    is_root: bool,
) {
    let indent = "  ".repeat(depth);
    let text = escape_xml(&node.text);
    let is_folded = node.folded && !is_root;
    let children: Vec<&Node> = map.children(node.id);
    let attr_folded = if is_folded { " FOLDED=\"true\"" } else { "" };
    if children.is_empty() {
        let _ = writeln!(out, "{}<node TEXT=\"{}\"{} />", indent, text, attr_folded);
    } else {
        let _ = writeln!(out, "{}<node TEXT=\"{}\"{}>", indent, text, attr_folded);
        for child in &children {
            write_freemind_node(out, child, map, depth + 1, false);
        }
        let _ = writeln!(out, "{}</node>", indent);
    }
}

fn empty_freemind() -> String {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<map version="1.0.1"/>
"#
    .to_string()
}

/// 从 OPML 1.0 XML 导入思维导图。
///
/// 解析逻辑：手写极简 XML 解析器，仅识别 `<outline .../>` 与
/// `<outline ...>...</outline>` 两种结构。折叠状态从 `_folded`
/// 属性读取（VeinMind 扩展）。
pub fn import_opml(opml: &str) -> Result<MindMap, String> {
    // 找到 <body> ... </body>
    let body_start = opml
        .find("<body>")
        .ok_or_else(|| "OPML missing <body>".to_string())?;
    let body_end = opml
        .find("</body>")
        .ok_or_else(|| "OPML missing </body>".to_string())?;
    let body = &opml[body_start + 6..body_end];

    // 标题
    let title = opml
        .find("<title>")
        .and_then(|s| opml[s + 7..].find("</title>").map(|e| opml[s + 7..s + 7 + e].to_string()))
        .unwrap_or_else(|| "Imported OPML".into());

    let mut map = MindMap::new(title);
    // 解析栈：每层是 (parent_id, 当前列出的子 outline)
    // 我们用递归下降解析 outline 树。
    let mut parser = OutlineParser::new(body);
    let root_node = parser.parse_root()?;
    let root_id = root_node.id;
    map.add_node(root_node);
    for child in parser.children {
        import_subtree(&mut map, child, Some(root_id));
    }
    map.set_root(root_id);
    Ok(map)
}

/// 极简 OPML outline 节点（中间态）。
struct OpmlNode {
    text: String,
    folded: bool,
    children: Vec<OpmlNode>,
}

/// 递归把 OpmlNode 树挂入 MindMap。
fn import_subtree(map: &mut MindMap, node: OpmlNode, parent_id: Option<uuid::Uuid>) {
    use uuid::Uuid;
    let id = Uuid::new_v4();
    let mut n = Node::new(id, node.text, parent_id);
    n.folded = node.folded;
    map.add_node(n);
    for child in node.children {
        import_subtree(map, child, Some(id));
    }
}

/// 手写极简 OPML outline 递归下降解析器。
struct OutlineParser<'a> {
    s: &'a str,
    pos: usize,
    children: Vec<OpmlNode>,
}

impl<'a> OutlineParser<'a> {
    fn new(s: &'a str) -> Self {
        Self { s, pos: 0, children: Vec::new() }
    }

    /// 解析顶层（body 内首个 outline 为根）。
    fn parse_root(&mut self) -> Result<Node, String> {
        use uuid::Uuid;
        // 跳到第一个 <outline
        let root = self.parse_outline()?;
        let id = Uuid::new_v4();
        let mut n = Node::new(id, root.text, None);
        n.folded = root.folded;
        // root 的 children 暂存到 parser.children
        self.children = root.children;
        Ok(n)
    }

    /// 解析一个 <outline .../> 或 <outline ...>...</outline>。
    fn parse_outline(&mut self) -> Result<OpmlNode, String> {
        // 找 <outline
        let start = self.s[self.pos..]
            .find("<outline")
            .ok_or_else(|| "expected <outline".to_string())?;
        let tag_start = self.pos + start;
        // 找标签结束 '>'（可能是自闭合 />）
        let tag_end_rel = self.s[tag_start..]
            .find('>')
            .ok_or_else(|| "unterminated <outline".to_string())?;
        let tag_end = tag_start + tag_end_rel;
        let tag_content = &self.s[tag_start + 8..tag_end]; // 去掉 "<outline"
        self.pos = tag_end + 1;

        // 解析 text 属性
        let text = extract_attr(tag_content, "text").unwrap_or_default();
        let folded = extract_attr(tag_content, "_folded")
            .map(|v| v == "true")
            .unwrap_or(false);

        let is_self_closing = self.s[..tag_end].ends_with('/');
        let children = if is_self_closing {
            Vec::new()
        } else {
            // 递归解析子 outline，直到遇到 </outline>
            let mut kids = Vec::new();
            loop {
                // 跳过空白
                let rest = &self.s[self.pos..];
                let trimmed_start = rest.find(|c: char| !c.is_whitespace()).unwrap_or(0);
                self.pos += trimmed_start;
                let rest = &self.s[self.pos..];
                if rest.starts_with("</outline>") {
                    self.pos += "</outline>".len();
                    break;
                }
                if rest.starts_with("<outline") {
                    kids.push(self.parse_outline()?);
                } else {
                    // 未知内容，跳一个字符防止死循环
                    self.pos += 1;
                    if self.pos >= self.s.len() {
                        break;
                    }
                }
            }
            kids
        };

        Ok(OpmlNode { text, folded, children })
    }
}

/// 从标签内容（如 `text="hi" _folded="false"`）提取属性值。
fn extract_attr(tag: &str, name: &str) -> Option<String> {
    let pat = format!("{}=\"", name);
    let start = tag.find(&pat)?;
    let val_start = start + pat.len();
    let end = tag[val_start..].find('"')?;
    Some(tag[val_start..val_start + end].to_string())
}

// =================================================================
//  FreeMind .mm 导入
// =================================================================
//
// 解析 FreeMind 的 XML 结构：
//   <map version="1.0.1">
//     <node TEXT="根节点" FOLDED="true">
//       <node TEXT="子节点"/>
//     </node>
//   </map>
// 与 OPML 的 `<outline>` 结构同构，故复用 [`OpmlNode`] 与
// [`import_subtree`] 挂树逻辑。

/// 从 FreeMind `.mm` XML 导入思维导图。
pub fn import_freemind(mm: &str) -> Result<MindMap, String> {
    let map_start = mm
        .find("<map")
        .ok_or_else(|| "FreeMind missing <map>".to_string())?;
    let map_end = mm
        .find("</map>")
        .ok_or_else(|| "FreeMind missing </map>".to_string())?;
    let body = &mm[map_start + 4..map_end];

    let mut parser = FreeMindParser::new(body);
    let root = parser.parse_node()?;

    let mut map = MindMap::new("Imported FreeMind".into());
    let root_id = uuid::Uuid::new_v4();
    let mut n = Node::new(root_id, root.text, None);
    n.folded = root.folded;
    map.add_node(n);
    for child in root.children {
        import_subtree(&mut map, child, Some(root_id));
    }
    map.set_root(root_id);
    Ok(map)
}

/// FreeMind `<node TEXT="...">` 递归下降解析器。
struct FreeMindParser<'a> {
    s: &'a str,
    pos: usize,
}

impl<'a> FreeMindParser<'a> {
    fn new(s: &'a str) -> Self {
        Self { s, pos: 0 }
    }

    /// 解析一个 `<node .../>` 或 `<node ...>...</node>`。
    fn parse_node(&mut self) -> Result<OpmlNode, String> {
        let start = self.s[self.pos..]
            .find("<node")
            .ok_or_else(|| "expected <node".to_string())?;
        let tag_start = self.pos + start;
        let tag_end_rel = self.s[tag_start..]
            .find('>')
            .ok_or_else(|| "unterminated <node".to_string())?;
        let tag_end = tag_start + tag_end_rel;
        let tag_content = &self.s[tag_start + 5..tag_end]; // 去掉 "<node"
        self.pos = tag_end + 1;

        let text = extract_attr(tag_content, "TEXT").unwrap_or_default();
        let folded = extract_attr(tag_content, "FOLDED")
            .map(|v| v == "true")
            .unwrap_or(false);

        let is_self_closing = self.s[..tag_end].ends_with('/');
        let children = if is_self_closing {
            Vec::new()
        } else {
            let mut kids = Vec::new();
            loop {
                let rest = &self.s[self.pos..];
                let trimmed_start = rest.find(|c: char| !c.is_whitespace()).unwrap_or(0);
                self.pos += trimmed_start;
                let rest = &self.s[self.pos..];
                if rest.starts_with("</node>") {
                    self.pos += "</node>".len();
                    break;
                }
                if rest.starts_with("<node") {
                    kids.push(self.parse_node()?);
                } else {
                    self.pos += 1;
                    if self.pos >= self.s.len() {
                        break;
                    }
                }
            }
            kids
        };

        Ok(OpmlNode { text, folded, children })
    }
}

#[cfg(test)]
mod opml_tests {
    use super::*;

    #[test]
    fn export_opml_basic() {
        let mut map = MindMap::new("测试".into());
        let root = Node::new(uuid::Uuid::new_v4(), "根".into(), None);
        let root_id = root.id;
        map.add_node(root);
        map.set_root(root_id);
        let child = Node::new(uuid::Uuid::new_v4(), "子".into(), Some(root_id));
        map.add_node(child);
        let xml = export_opml(&map);
        assert!(xml.contains("<opml version=\"1.0\">"));
        assert!(xml.contains("根"));
        assert!(xml.contains("子"));
    }

    #[test]
    fn import_opml_roundtrip() {
        let opml = r##"<?xml version="1.0" encoding="UTF-8"?>
<opml version="1.0">
<head><title>RT</title></head>
<body>
<outline text="根">
<outline text="子1"/>
<outline text="子2">
<outline text="孙"/>
</outline>
</outline>
</body>
</opml>"##;
        let map = import_opml(opml).unwrap();
        assert_eq!(map.title, "RT");
        assert!(map.root_id.is_some());
        // 根 + 子1 + 子2 + 孙 = 4
        assert_eq!(map.nodes.len(), 4);
    }
}

