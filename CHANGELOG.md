# Changelog

本项目所有显著变更都将记录在此文件中。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 新增
- 核心节点 CRUD、父子层级与折叠/展开
- 拖拽交互：节点吸附、子树整体跟随、拖拽改层级、自由坐标持久化
- 撤销 / 重做（Command 模式线性历史栈，`Ctrl+Z` / `Ctrl+Y`）
- 四种布局策略：横向树 `tree`、逻辑图 `logic`、放射状 `radial`、紧凑大纲 `compact`
- 6 款主题：经典亮色 / 经典暗色 / 海盐青 / 暖阳琥珀 / 墨绿森林 / 午夜紫
- 属性面板：填充色、文字色、图标、字体样式、字号
- 富文本节点（Markdown 子集：粗体/斜体/链接/代码/标题/列表/多行）
- 关系线（`Shift+点击` 创建、右键删除）
- 漂浮主题（游离节点布局到主树外侧）
- 搜索 / 跳转（`Ctrl+F`，匹配高亮 + Enter 循环跳转）
- 小地图（缩略图 + 视口框，随主题联动配色）
- 多文档管理：最近文件列表（最多 8 条）、文档标题编辑（可撤销）
- JSON 本地保存 / 打开 / 自动落盘（去抖 + 临时文件原子写 + 退出前 flush）
- 导入 / 导出：Markdown、SVG、PNG、OPML、FreeMind（.mm）导出；OPML / FreeMind 导入
- EventBus 事件通知（Observer 模式）

[Unreleased]: https://atomgit.com/
