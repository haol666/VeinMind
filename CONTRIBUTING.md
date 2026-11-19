# 贡献指南

感谢你对 VeinMind 的关注！欢迎以任何形式贡献：报告缺陷、提交功能建议、完善文档或直接提交代码。

## 行为准则

请保持友善、尊重与专业，共同维护开放包容的社区氛围。

## 如何贡献

### 报告缺陷

提交 Issue 前请先搜索是否已有重复报告。Issue 中请尽量包含：

- 操作系统与版本（以及 Tauri / Rust 版本）
- 复现步骤（越具体越好）
- 期望行为与实际行为
- 相关日志或截图

### 提交功能建议

请描述使用场景与期望效果；如果涉及 UI，附上草图或参考截图更有帮助。

### 提交代码

1. Fork 仓库并创建特性分支：

   ```bash
   git checkout -b feat/your-feature
   ```

2. 完成开发并自测通过（详见下文「开发环境」）。
3. 提交 Pull Request，说明改动动机、方案与验证方式。
4. 等待 Review，根据意见修改直至合并。

## 开发环境

| 依赖 | 版本 |
|------|------|
| Rust | 1.75+ |
| Tauri CLI | 1.6.x |
| 前端 | 原生 HTML5 / CSS3 / Canvas 2D，无需构建工具 |

常用命令：

```bash
# 调试运行
cargo tauri dev

# 构建发布版本
cargo tauri build

# 检查 / 测试
cargo check
cargo test
```

## 代码约定

- 严格遵循项目既有的**分层架构**与 **GoF 设计模式**，保持高内聚、低耦合。
- 提交前运行 `cargo fmt` 与 `cargo clippy`，确保无警告。
- 新增功能请附带必要的测试与文档说明（同步更新 README / CHANGELOG）。
- 提交信息使用 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/) 规范，例如：

  ```
  feat(layout): 支持紧凑大纲布局的长文本换行
  fix(render): 修复放射状布局下漂浮节点重叠问题
  ```

## License

提交即表示你同意以 [MIT License](LICENSE) 发布你的贡献。
