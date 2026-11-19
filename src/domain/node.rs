//! 思维导图节点实体。
//!
//! 采用 **不可变值 + 聚合内方法** 的风格：`Node` 是纯数据，所有变更
//! 都走 [`crate::application::command`] 的命令对象，以保证可撤销。
//!
//! 富文本：`Node.text` 存储原始 Markdown 子集字符串（支持多行、
//! `**bold**`、`*italic*`、`[link](url)`、\`code\`、`#`~`###` 标题）。
//! 渲染层与导出层各自负责把 Markdown 子集转为目标格式（HTML/SVG/纯文本）。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 节点视觉样式。`Option` 字段表示“未设置则继承父节点/默认值”。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeStyle {
    pub bg_color: Option<String>,
    pub fg_color: Option<String>,
    pub font_size: Option<u32>,
    pub bold: Option<bool>,
    /// 斜体（文字样式）。
    #[serde(default)]
    pub italic: Option<bool>,
    /// 下划线（文字样式）。
    #[serde(default)]
    pub underline: Option<bool>,
    /// 删除线（文字样式）。
    #[serde(default)]
    pub strikethrough: Option<bool>,
    pub icon: Option<String>,
}

/// 思维导图节点。节点之间通过 `parent_id` 形成树形层级，
/// `order` 字段用于在同层兄弟中保持插入顺序。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
    /// 原始富文本（Markdown 子集，可含换行）。空字符串视为占位。
    pub text: String,
    /// 节点备注：附加的长文本（隐藏在主文本之后，市面工具标配 Notes）。
    /// `#[serde(default)]` 保证旧版 JSON 文件（无此字段）可正常反序列化。
    #[serde(default)]
    pub notes: String,
    pub parent_id: Option<Uuid>,
    pub order: u32,
    pub folded: bool,
    pub style: NodeStyle,
    /// 自由坐标（用户拖拽吸附后的持久化位置）。
    /// `Some` 时布局引擎优先使用此坐标，不再被算法布局拉回。
    /// `#[serde(default)]` 保证旧版 JSON 文件（无此字段）可正常反序列化。
    #[serde(default)]
    pub free_x: Option<f32>,
    #[serde(default)]
    pub free_y: Option<f32>,
}

impl Node {
    /// 工厂构造方法（**Factory 模式** 雏形）。
    pub fn new(id: Uuid, text: String, parent_id: Option<Uuid>) -> Self {
        Self {
            id,
            text,
            notes: String::new(),
            parent_id,
            order: 0,
            folded: false,
            style: NodeStyle::default(),
            free_x: None,
            free_y: None,
        }
    }

    /// 判断是否为根节点（无父节点）。
    pub fn is_root(&self) -> bool {
        self.parent_id.is_none()
    }

    /// 设置自由坐标（拖拽吸附终点持久化）。
    pub fn set_free_pos(&mut self, x: f32, y: f32) {
        self.free_x = Some(x);
        self.free_y = Some(y);
    }

    /// 清除自由坐标（例如移动子树后恢复算法布局）。
    pub fn clear_free_pos(&mut self) {
        self.free_x = None;
        self.free_y = None;
    }

    /// 返回自由坐标，未设置时返回 None。
    pub fn free_pos(&self) -> Option<(f32, f32)> {
        match (self.free_x, self.free_y) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => None,
        }
    }

    /// 设置同层排序序号。
    pub fn set_order(&mut self, order: u32) {
        self.order = order;
    }

    /// 折叠/展开切换。
    pub fn toggle_fold(&mut self) {
        self.folded = !self.folded;
    }

    /// 应用新文本。
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    /// 合并样式（仅覆盖已提供的字段，**Builder 思想** 的简化版）。
    ///
    /// 字符串字段约定：**空字符串表示清除该样式**（设为 `None`）。
    /// 前端"取消填充颜色/取消图标"传空字符串即可生效——若直接传 `null`
    /// 会被 serde 反序列化为 `None`，从而不覆盖已有值（清除无效的根因）。
    pub fn merge_style(&mut self, other: &NodeStyle) {
        if let Some(v) = &other.bg_color {
            self.style.bg_color = if v.is_empty() { None } else { Some(v.clone()) };
        }
        if let Some(v) = &other.fg_color {
            self.style.fg_color = if v.is_empty() { None } else { Some(v.clone()) };
        }
        if other.font_size.is_some() { self.style.font_size = other.font_size; }
        if other.bold.is_some() { self.style.bold = other.bold; }
        if other.italic.is_some() { self.style.italic = other.italic; }
        if other.underline.is_some() { self.style.underline = other.underline; }
        if other.strikethrough.is_some() { self.style.strikethrough = other.strikethrough; }
        if let Some(v) = &other.icon {
            self.style.icon = if v.is_empty() { None } else { Some(v.clone()) };
        }
    }

    // ---------- 富文本辅助 ----------

    /// 返回纯文本表示（剥离 Markdown 标记，保留换行）。
    /// 供布局度量、大纲导出、搜索索引使用。
    pub fn plain_text(&self) -> String {
        strip_markdown(&self.text)
    }

    /// 返回行数（用于布局高度计算）。至少 1 行。
    pub fn line_count(&self) -> usize {
        self.text.lines().count().max(1)
    }

    /// 返回最长行的字符数（用于布局宽度估算）。
    pub fn max_line_chars(&self) -> usize {
        self.text
            .lines()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0)
    }

    /// 是否包含 Markdown 富文本标记。
    pub fn has_markdown(&self) -> bool {
        let t = &self.text;
        t.contains("**")
            || t.contains('*')
            || t.contains("](")
            || t.contains('`')
            || t.contains("\n#")
            || t.starts_with('#')
    }
}

/// 剥离 Markdown 子集标记，返回纯文本。
///
/// 支持的子集：
///   - `**bold**`、`*italic*`
///   - `[label](url)` → `label`
///   - \`code\` → `code`（去反引号）
///   - 行首 `#`~`###` 标题前缀
///   - 行首 `-` / `*` / `1.` 列表标记
fn strip_markdown(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for raw_line in s.lines() {
        let mut line = raw_line;
        // 标题前缀
        line = line.trim_start_matches('#').trim_start();
        // 列表标记
        if let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            line = rest;
        } else if let Some(rest) = line.strip_prefix("-").or_else(|| line.strip_prefix("*")) {
            if rest.is_empty() || rest.starts_with(' ') {
                line = rest.trim_start();
            }
        }
        // 行内：bold/italic/link/code
        // 用字节索引 i 推进；所有匹配失败的分支必须保证 i 至少前进一个字符。
        let mut cleaned = String::with_capacity(line.len());
        let mut i = 0usize;
        let bytes = line.as_bytes();
        while i < bytes.len() {
            // **bold**
            if line[i..].starts_with("**") {
                if let Some(end) = line[i + 2..].find("**") {
                    cleaned.push_str(&line[i + 2..i + 2 + end]);
                    i += 2 + end + 2;
                    continue;
                }
            }
            // *italic*
            if line[i..].starts_with('*') {
                if let Some(end) = line[i + 1..].find('*') {
                    cleaned.push_str(&line[i + 1..i + 1 + end]);
                    i += 1 + end + 1;
                    continue;
                }
            }
            // `code`
            if line[i..].starts_with('`') {
                if let Some(end) = line[i + 1..].find('`') {
                    cleaned.push_str(&line[i + 1..i + 1 + end]);
                    i += 1 + end + 1;
                    continue;
                }
            }
            // [label](url)
            if line[i..].starts_with('[') {
                if let Some(label_end) = line[i + 1..].find(']') {
                    if line[i + 1 + label_end + 1..].starts_with('(') {
                        if let Some(url_end) = line[i + 1 + label_end + 2..].find(')') {
                            cleaned.push_str(&line[i + 1..i + 1 + label_end]);
                            i += 1 + label_end + 2 + url_end + 1;
                            continue;
                        }
                    }
                }
            }
            // 普通字符：用 line[i..] 的第一个字符，保证 i 按字符宽度推进。
            // line[i..] 一定是 char 边界（i 从 0 开始且只在 char 边界处增加）。
            let next_ch = line[i..].chars().next();
            match next_ch {
                Some(c) => {
                    cleaned.push(c);
                    i += c.len_utf8();
                }
                None => {
                    // 理论不可达（i < bytes.len() 时 line[i..] 至少有一个字符）
                    break;
                }
            }
        }
        out.push_str(&cleaned);
        out.push('\n');
    }
    // 去末尾多余换行
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_bold_italic() {
        assert_eq!(strip_markdown("**hi** *world*"), "hi world");
    }

    #[test]
    fn strip_link() {
        assert_eq!(strip_markdown("[docs](https://x)"), "docs");
    }

    #[test]
    fn strip_code() {
        assert_eq!(strip_markdown("run `cargo build`"), "run cargo build");
    }

    #[test]
    fn multiline_plain() {
        let n = Node::new(Uuid::new_v4(), "行一\n**行二**".into(), None);
        assert_eq!(n.plain_text(), "行一\n行二");
        assert_eq!(n.line_count(), 2);
    }

    #[test]
    fn heading_strip() {
        assert_eq!(strip_markdown("# Title"), "Title");
        assert_eq!(strip_markdown("### 深"), "深");
    }

    #[test]
    fn list_marker_strip() {
        assert_eq!(strip_markdown("- a\n* b"), "a\nb");
    }
}
