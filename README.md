# VeinMind · 思维导图工具

> **市面思维导图工具之大成，以 Rust + Tauri 为基座，严格遵循分层架构与经典设计模式。**

VeinMind 是一款桌面端思维导图编辑器，吸收了 XMind、MindNode、MindMeister、FreeMind、Workflowy、幕布 等工具的核心能力，采用 **「精致生产力」(Refined Productivity)** 设计语言，并用软件工程学的分层架构 + GoF 设计模式加以落地，使代码具备**高内聚、低耦合、易扩展**的特质。

---

## 一、功能总览

| 能力 | 说明 |
|------|------|
| 节点 CRUD | 增删改查节点，父子层级，折叠/展开 |
| **拖拽交互** | 节点拖拽吸附鼠标坐标、**子树整体跟随**、拖拽改层级、自由坐标持久化（不回弹） |
| 撤销 / 重做 | 线性历史栈（Command 模式），支持 `Ctrl+Z` / `Ctrl+Y` |
| **四种布局策略** | 横向树 `tree`、逻辑图 `logic`、放射状 `radial`、紧凑大纲 `compact`（均感知文本宽度，长文本/多行节点不重叠） |
| **6 款主题** | 经典亮色 / 经典暗色 / 海盐青 / 暖阳琥珀 / 墨绿森林 / 午夜紫，明暗家族共享令牌 |
| **属性面板** | 选中节点滑出：填充色、文字色、图标、加粗/斜体/下划线/删除线、字号 |
| **富文本节点** | Markdown 子集（粗体/斜体/链接/代码/标题/列表/多行），图标水平居左排列 |
| **关系线** | 跨节点自定义连接（Shift+点击创建、右键删除），虚线高亮 |
| **漂浮主题** | 游离节点不挂树，布局到主树外侧 |
| **搜索 / 跳转** | Ctrl+F 浮动搜索栏，匹配高亮 + Enter 循环跳转 |
| **小地图** | 右下角独立组件，缩略图 + 视口框，随主题联动配色 |
| **多文档管理** | 最近文件列表（localStorage，最多 8 条）、文档标题可编辑（可撤销） |
| 持久化 | JSON 本地保存 / 打开 / **自动落盘**（去抖 500ms + 临时文件原子写 + 退出前 flush 兜底） |
| **导入 / 导出** | Markdown、SVG、PNG、OPML、FreeMind（.mm）导出；OPML / FreeMind 导入 |
| 事件通知 | EventBus 广播精确领域事件（Observer 模式，事件携带真实实体 id） |

### 快捷键

| 键位 | 动作 |
|------|------|
| `Enter` / `Tab` | 添加子节点 |
| `Delete` / `Backspace` | 删除选中节点 |
| `Space` | 折叠 / 展开 |
| `Ctrl+Z` / `Ctrl+Y` | 撤销 / 重做 |
| `Ctrl+F` | 搜索 / 跳转 |
| `Ctrl+S` / `Ctrl+O` | 保存 / 打开 |
| 双击节点 | 内联编辑（Enter 提交 / Shift+Enter 换行 / Esc 取消） |
| 右键节点 / 关系线 | 上下文菜单（增删、编辑、折叠、删除关系线） |
| `Shift+点击` 另一节点 | 创建关系线 |
| 拖拽节点 | 吸附鼠标 + 子树跟随；拖到另一节点上改层级 |

---

## 二、技术栈

| 层 | 技术 |
|----|------|
| 后端核心 | Rust 1.75+ |
| 桌面框架 | Tauri 1.6 |
| 序列化 | serde + serde_json |
| 前端 | 原生 HTML5 / CSS3 / Canvas 2D（零构建依赖） |
| UI 设计 | 「Refined Productivity」设计语言，CSS 语义令牌 + 6 主题 |
| 日志 | env_logger |

---

## 三、项目结构

```
VeinMind/
├── Cargo.toml                # Rust 包定义与依赖
├── build.rs                  # Tauri 构建脚本
├── tauri.conf.json           # Tauri 应用配置
├── .gitignore                # git 忽略规则
├── README.md                 # 项目说明文档
├── design/                   # UI 设计文件（高保真原型 + 设计规范）
├── src/
│   ├── main.rs               # 二进制入口
│   ├── lib.rs                # 库入口 + AppState 装配
│   ├── domain/               # ── 领域层（纯业务，零外部依赖）
│   │   ├── mod.rs
│   │   ├── node.rs           # Node 实体 + NodeStyle（含自由坐标/文本装饰）
│   │   ├── edge.rs           # Edge 关系线实体
│   │   ├── mind_map.rs       # MindMap 聚合根（含 node_index 索引 + children_map）
│   │   └── visitor.rs        # NodeVisitor trait + DFS/BFS 遍历
│   ├── application/          # ── 应用层（用例编排）
│   │   ├── mod.rs
│   │   ├── command.rs        # Command 模式：撤销/重做栈 + 15 种具体命令
│   │   ├── event_bus.rs      # Observer 模式：EventBus + 精确领域事件
│   │   └── layout.rs         # Strategy 模式：4 布局（均感知文本宽度）
│   ├── infrastructure/       # ── 基础设施层（技术细节）
│   │   ├── mod.rs
│   │   ├── persistence.rs    # Repository 模式：去抖自动保存 + 原子写 + flush 兜底
│   │   └── exporter.rs       # Visitor 模式：5 种导出 + 2 种导入
│   ├── commands/             # ── 表示层（Tauri IPC bridge）
│   │   ├── mod.rs
│   │   ├── dto.rs            # 数据传输对象（含 warning 非致命警告字段）
│   │   └── mindmap.rs        # 24 个 #[tauri::command] 实现
│   └── integration_tests.rs  # 50 个端到端集成测试
├── frontend/                 # ── 表示层（静态前端）
│   ├── index.html            # app-shell 四区布局：顶栏 + 左工具栏 + 画布 + 底栏
│   ├── styles.css            # 设计令牌（6 主题）+ 全部组件样式
│   └── app.js                # Canvas 渲染 + 交互 + invoke 调用
└── icons/                    # 应用图标（Tauri 构建资源）
```

---

## 四、分层架构

VeinMind 采用经典的 **四层架构**，依赖方向严格自上而下，领域层零外部依赖。

```
┌─────────────────────────────────────────────────────────┐
│  Presentation Layer  表示层                              │
│  ─ frontend/ (HTML + Canvas + 6 主题)                    │
│  ─ commands/   (Tauri IPC，24 个命令)                    │
│  职责：渲染、交互、参数解包、DTO 封装                     │
├─────────────────────────────────────────────────────────┤
│  Application Layer  应用层                               │
│  ─ command.rs   Command 模式（undo/redo + 自由坐标）     │
│  ─ event_bus.rs Observer 模式（事件广播）                │
│  ─ layout.rs    Strategy 模式（4 布局 + 自由坐标覆盖）    │
│  职责：用例编排，调度领域层与基础设施层                  │
├─────────────────────────────────────────────────────────┤
│  Domain Layer  领域层                                    │
│  ─ MindMap / Node / Edge  聚合根与实体                   │
│  ─ NodeVisitor            遍历抽象                       │
│  职责：纯粹的业务模型与不变量守护                         │
├─────────────────────────────────────────────────────────┤
│  Infrastructure Layer  基础设施层                        │
│  ─ persistence.rs  Repository 模式（JSON 仓储）          │
│  ─ exporter.rs     Visitor 模式（5 种导出 + 2 种导入）    │
│  职责：技术细节落地：I/O、序列化、导出                   │
└─────────────────────────────────────────────────────────┘
```

### 设计原则映射

| 原则 | 体现 |
|------|------|
| **单一职责 (SRP)** | 每层只做一件事；每个 Command 只封装一个操作 |
| **开闭原则 (OCP)** | 新增布局策略 / 导出格式 / 主题无需改动现有类 |
| **里氏替换 (LSP)** | 任何 `LayoutStrategy` 实现均可替换 `TreeLayout` |
| **接口隔离 (ISP)** | `NodeVisitor` 只暴露访问方法，不强迫实现者关心存储 |
| **依赖倒置 (DIP)** | 应用层依赖 `MindMapRepository` trait，而非具体 JSON 实现 |

---

## 五、设计模式落地

### 5.1 Command 模式 — 可撤销编辑

**文件**：`src/application/command.rs`

将每个用户操作封装为独立的命令对象，实现 `Command` trait：

```rust
pub trait Command: Send + Sync {
    fn name(&self) -> &'static str;
    fn execute(&mut self, map: &mut MindMap);
    fn undo(&mut self, map: &mut MindMap);
    /// 本次执行新创建实体的 id（供发布精确领域事件）
    fn created_id(&self) -> Option<Uuid> { None }
}

pub struct CommandInvoker {
    undo_stack: VecDeque<Box<dyn Command>>,
    redo_stack: VecDeque<Box<dyn Command>>,
    capacity: usize,          // 快照上限，防止无界内存增长
}
```

`CommandInvoker` 维护两个栈：`undo` 弹栈并调用 `command.undo()`，同时把它压入 `redo_stack`；新命令执行后 `redo_stack` 清空。这是经典的**线性历史模型**。`execute` 返回命令新创建实体的 id，供上层发布**精确**的领域事件。

当前共 **15 种命令**：

| 命令 | 说明 | 撤销方式 |
|------|------|---------|
| `AddChildNode` | 添加子节点 | 删除创建节点 |
| `AddSiblingNode` | 添加兄弟节点 | 删除创建节点 |
| `AddFreeNode` | 添加漂浮主题 | 删除创建节点 |
| `UpdateNodeText` | 修改节点文本 | 恢复旧文本 |
| `UpdateNodeStyle` | 修改节点样式 | 恢复旧样式 |
| `UpdateNodeNotes` | 修改节点备注 | 恢复旧备注 |
| `UpdateDocumentTitle` | 修改文档标题 | 恢复旧标题 |
| `MoveNode` | 移动子树（清除自由坐标） | 恢复父节点 |
| `RemoveNode` | 删除子树 | **Memento 快照**整体还原 |
| `ToggleFold` | 折叠/展开 | 再次切换 |
| `AddEdge` | 新增关系线 | 删除关系线 |
| `RemoveEdge` | 删除关系线 | 重新添加 |
| `SetNodeFreePos` | 拖拽终点持久化 | 清除自由坐标 |
| `SetLayoutStrategy` | 切换布局策略 | — |

### 5.2 Observer 模式 — 事件广播

**文件**：`src/application/event_bus.rs`

`EventBus` 充当事件中心，业务层在节点变更后 `publish` 事件，订阅者（日志、自动保存、UI 刷新等）通过 `subscribe` 注册回调。**事件携带真实实体 id**（如新节点 id、关系线 id），Observer 链真实可用。

```rust
pub enum DomainEvent {
    DocumentReset { title },                  // 文档新建/加载
    DocumentTitleUpdated { title },           // 标题更新
    NodeAdded { node_id, parent_id },         // 节点新增（真实 id）
    NodeRemoved { node_id },                  // 节点删除
    NodeTextUpdated { node_id, old, new },    // 文本更新
    NodeStyleUpdated { node_id },             // 样式更新
    NodeNotesUpdated { node_id },             // 备注更新
    NodeFoldToggled { node_id, folded },      // 折叠/展开
    NodeMoved { node_id, new_parent_id },     // 子树移动
    NodeFreePosSet { node_id },               // 自由坐标持久化
    EdgeAdded { edge_id, source, target },    // 关系线新增（真实 id）
    EdgeRemoved { edge_id },                  // 关系线删除
    HistoryChanged { can_undo, can_redo },    // 历史变化
}
```

启动时注册两个订阅者：**结构化日志** + **自动保存触发**（覆盖全部 11 种变更事件）。

### 5.3 Strategy 模式 — 可插拔布局

**文件**：`src/application/layout.rs`

把"如何给节点排坐标"抽象为一族可互换的布局策略，均感知**文本实际宽度**（多行/长文本节点不再重叠）：

| 策略 | 说明 |
|------|------|
| `TreeLayout` | 经典横向树，根在左、子向右展开（x 随文本宽度推进） |
| `LogicLayout` | 左右双向树，一级子节点奇偶交替分布（XMind 逻辑图风格） |
| `RadialLayout` | 放射状，根居中、子按角度向外 |
| `CompactLayout` | 紧凑纵向树，适合大纲浏览 |

所有策略复用 `MindMap::children_map()` 一次性构建的邻接表，布局整体 **O(n)**（原 O(n²)）。`LayoutEngine::compute` 最后应用**自由坐标覆盖**：用户拖拽吸附的终点优先于算法布局。

### 5.4 Visitor 模式 — 导出器

**文件**：`src/infrastructure/exporter.rs` + `src/domain/visitor.rs`

将"对节点树的操作"从"节点数据结构"中解耦。新增导出格式只需新增一个 `NodeVisitor` 实现，无需修改 `Node`/`MindMap`。

| 访问者 | 产物 |
|--------|------|
| `MarkdownExporter` | Markdown（嵌套列表） |
| `OutlineExporter` | 纯文本大纲 |
| `export_svg` | SVG 矢量图（真实 `<rect>`/`<text>`/`<path>`） |
| `export_opml` / `import_opml` | OPML 大纲（双向，含往返一致性测试） |
| `export_freemind` / `import_freemind` | FreeMind `.mm`（双向） |

### 5.5 Repository 模式 — 持久化

**文件**：`src/infrastructure/persistence.rs`

`MindMapRepository` trait 定义持久化抽象，`JsonMindMapRepository` 为其具体实现。切换为 SQLite 等后端只需新增一个实现，上层代码无感——符合**依赖倒置原则**。

自动保存采用 **去抖 + 原子写 + 兜底 flush** 三重保障：

| 机制 | 说明 |
|------|------|
| **去抖 (500ms)** | 窗口内多次 `store` 只写盘一次（内存始终即时更新，磁盘按窗口合并），避免每次按键都做全量序列化 + 写盘 |
| **原子写** | 先写同目录 `.json.tmp` 再 rename，避免中途崩溃留下半截损坏的 JSON |
| **flush 兜底** | 退出前强制落盘，保证关闭应用不丢数据（P0-2） |

`node_index` 不参与序列化（`#[serde(skip)]`），反序列化/删除节点后调用 `rebuild_index()` 重建。

---

## 六、性能优化

| 优化 | 位置 | 效果 |
|------|------|------|
| **id 索引** | `MindMap::node_index: HashMap<Uuid, usize>` | `get_node`/`get_node_mut` O(1)，替代线性扫描 |
| **邻接表复用** | `MindMap::children_map()` | 布局/后代遍历 O(n²) → O(n) |
| **删除用 HashSet** | `remove_subtree` | `contains` 查询 O(1) |
| **命令链路去克隆** | `build_dto_from(state, map)` 直接消费已加载 map | 每次编辑少 1~2 次全图克隆 |
| **自动保存去抖** | `AUTOSAVE_DEBOUNCE = 500ms` | 避免每按键写全文件 |
| **原子写** | 临时文件 + rename | 防崩溃损坏 |
| **文本感知布局** | `estimate_node_size()` 与前端 `measureNode` 规则对齐 | 长文本/多行节点不重叠 |

---

## 七、健壮性设计

| 设计 | 说明 |
|------|------|
| **非致命警告上报** | `DocumentDto::warning` 字段承载保存失败等提示，前端 Toast 感知（不丢数据） |
| **精确领域事件** | 事件携带真实实体 id（`EdgeAdded`/`NodeStyleUpdated` 等），Observer 链真实可用 |
| **环防护** | `move_subtree` 检测后代，拒绝把父节点移到子节点下 |
| **关系线去重** | 同对端点只保留一条关系线 |
| **命令容量上限** | `CommandInvoker::capacity` 防止撤销栈无界增长 |
| **错误传播** | 不可写路径的 `flush` 返回 `Err`，经 warning 字段上报 |

---

## 八、测试

共 **50 个端到端集成测试**，覆盖领域层、命令模式、持久化、导出、布局全部策略。

```bash
cargo test
```

关键测试场景：

| 测试 | 覆盖 |
|------|------|
| `command_undo_remove_with_subtree` | 删除子树 + 撤销（Memento 快照还原） |
| `command_add_remove_edge` | 关系线增删 + 去重 + 撤销 |
| `command_undo_move_node` | 移动节点 + 撤销 |
| `move_node_clears_free_pos` | 移动后自由坐标清除 |
| `layout_free_pos_priority` | 自由坐标优先于算法布局 |
| `layout_{tree,radial,compact,logic}_covers_all_nodes` | 四种布局覆盖全部节点 |
| `persistence_flush_writes_atomic_file` | 原子写 + flush 兜底 |
| `persistence_store_error_surfaces_as_warning` | 不可写路径报错 |
| `persistence_from_json_replaces_document` | 反序列化重建索引 |
| `export_svg_uses_layout_coordinates` | SVG 使用真实布局坐标 |
| `export_opml_roundtrip` / `import_freemind` | OPML/FreeMind 往返一致性 |
| `style_clear_via_empty_string` | 空字符串清除样式（bg/fg/icon） |
| `e2e_undo_restores_state_and_exports` | 撤销后导出还原 |

---

## 九、运行

### 前置条件

- Rust 1.75+ (`rustup`)
- Tauri 系统依赖（WebKitGTK / WebView2，见 [Tauri 官方文档](https://tauri.app/v1/guides/getting-started/prerequisites)）

### 开发与运行

```bash
# 克隆
git clone git@atomgit.com:mark_ningning/VeinMind.git
cd VeinMind

# 编译运行（开发模式）
cargo tauri dev

# 仅编译检查
cargo check

# 运行测试
cargo test

# 发布构建
cargo tauri build
```

---

## 十、路线图

### 已完成 ✅
- 四层分层架构 + 5 种 GoF 设计模式
- 节点 CRUD / 拖拽 / 撤销重做 / 折叠 / 漂浮主题
- 4 种布局策略（文本感知）+ 自由坐标持久化
- 6 款主题 + 属性面板 + 富文本节点
- 关系线 + 搜索 + 小地图 + 多文档管理
- 5 种导出 + 2 种导入（OPML / FreeMind）
- 自动保存（去抖 + 原子写 + flush 兜底）
- id 索引 + 邻接表 + 命令链路去克隆
- 50 个集成测试

### 近期可做 📋
- 前端 rAF 渲染节流 + 文本尺寸缓存
- 搜索增强（正则 / 替换 / 范围过滤）
- 复制/剪切/粘贴 + 多选
- 导出整图 PNG（当前仅导出视口）
- SQLite 仓储实现（依赖倒置已预留）
- 分支彩虹色 / 图片附件

---

## 许可证

MIT © VeinMind Contributors
