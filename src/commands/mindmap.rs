//! Tauri 命令实现 — 思维导图 CRUD + 撤销/重做 + 导出。
//!
//! 每个变更命令：
//!   1. 从 `State<AppState>` 取出共享状态；
//!   2. 构造对应的 `Command` 对象，经 **共享的** `CommandInvoker` 执行
//!      （invoker 持久存于 `AppState`，保证撤销栈跨调用存活）；
//!   3. 把执行后的文档写回 repository；
//!   4. 通过 `EventBus` 发布领域事件；
//!   5. 返回最新的 `DocumentDto` 给前端。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::State;
use uuid::Uuid;

use crate::application::command::{
    add_child_command, add_edge_command, add_free_node_command, add_sibling_command,
    move_node_command, remove_edge_command, remove_node_command, set_free_pos_command,
    toggle_fold_command, update_notes_command, update_style_command, update_text_command,
    update_title_command, Command,
};
use crate::application::event_bus::{DomainEvent, EventBus};
use crate::domain::{MindMap, Node, NodeStyle};
use crate::infrastructure::exporter;
use crate::infrastructure::persistence::MindMapRepository;

use super::dto::DocumentDto;

// =================================================================
//  内部辅助
// =================================================================

/// 读取当前文档，计算布局，组装 DTO。
fn snapshot(state: &State<'_, crate::AppState>) -> DocumentDto {
    build_dto(state, None)
}

/// 组装 DTO，可附带非致命警告（如自动保存失败）。
fn build_dto(state: &State<'_, crate::AppState>, warning: Option<String>) -> DocumentDto {
    let map = state
        .repo
        .load()
        .unwrap_or_else(|_| MindMap::new("Error".into()));
    build_dto_from(state, map, warning)
}

/// 用**已加载的 map** 直接组装 DTO（避免 `run_command` 里反复 `load()` 克隆整图）。
fn build_dto_from(
    state: &State<'_, crate::AppState>,
    map: MindMap,
    warning: Option<String>,
) -> DocumentDto {
    let layout = state.layout.lock().unwrap().compute(&map);
    let inv = state.invoker.lock().unwrap();
    DocumentDto {
        mind_map: map,
        layout,
        can_undo: inv.can_undo(),
        can_redo: inv.can_redo(),
        warning,
    }
}

/// 在共享 invoker 上执行一个命令，并把结果写回 repository + 发布事件。
///
/// `event` 是事件构造闭包：传入执行后的 `MindMap` 与命令创建实体 id，
/// 以便发布**精确**的领域事件（如真实的新节点 id、EdgeAdded/EdgeRemoved）。
fn run_command(
    state: &State<'_, crate::AppState>,
    cmd: Box<dyn Command>,
    event: impl FnOnce(&MindMap, Option<Uuid>) -> DomainEvent,
) -> DocumentDto {
    let mut map = match state.repo.load() {
        Ok(m) => m,
        Err(e) => return build_dto(state, Some(format!("读取文档失败: {e}"))),
    };
    let created = {
        let mut inv = state.invoker.lock().unwrap();
        inv.execute(cmd, &mut map)
    };
    let warning = match state.repo.store(map.clone()) {
        Ok(()) => None,
        Err(e) => Some(format!("自动保存失败: {e}")),
    };
    publish(state.bus.clone(), event(&map, created));
    build_dto_from(state, map, warning)
}

fn publish(bus: Arc<Mutex<EventBus>>, event: DomainEvent) {
    if let Ok(b) = bus.lock() {
        b.publish(&event);
    }
}

// =================================================================
//  文档级命令
// =================================================================

/// 返回当前文档快照（含布局）。
#[tauri::command]
pub fn get_document(state: State<'_, crate::AppState>) -> DocumentDto {
    snapshot(&state)
}

/// 新建空白文档（含一个根节点）。清空撤销/重做历史。
#[tauri::command]
pub fn create_document(
    title: Option<String>,
    state: State<'_, crate::AppState>,
) -> DocumentDto {
    let title = title.unwrap_or_else(|| "未命名".into());
    let mut map = MindMap::new(title.clone());
    let root = Node::new(Uuid::new_v4(), "Central Topic".into(), None);
    map.add_node(root.clone());
    map.set_root(root.id);

    let warning = match state.repo.store(map.clone()) {
        Ok(()) => None,
        Err(e) => Some(format!("自动保存失败: {e}")),
    };
    state.invoker.lock().unwrap().clear();
    publish(
        state.bus.clone(),
        DomainEvent::DocumentReset { title },
    );
    build_dto_from(&state, map, warning)
}

/// 保存当前文档为 JSON 字符串（由前端写入磁盘）。
///
/// 若之前调用过 [`bind_doc_path`]，repository 已自动落盘；此命令仍返回
/// 完整 JSON，供"另存为"使用。
#[tauri::command]
pub fn save_document(state: State<'_, crate::AppState>) -> Result<String, String> {
    state.repo.to_json().map_err(|e| e.to_string())
}

/// 绑定磁盘路径。之后的每次变更会自动写回此路径（自动保存）。
///
/// 前端在"保存/另存为"成功写盘后调用此命令，把文件路径注册到后端，
/// 这样后续编辑无需手动保存也不会丢数据。
#[tauri::command]
pub fn bind_doc_path(
    path: String,
    state: State<'_, crate::AppState>,
) -> bool {
    let p = PathBuf::from(&path);
    state.repo.set_path(p.clone());
    *state.doc_path.lock().unwrap() = Some(p);
    true
}

/// 从 JSON 字符串加载文档（替换当前文档）。清空历史。
#[tauri::command]
pub fn load_document(
    json: String,
    state: State<'_, crate::AppState>,
) -> DocumentDto {
    let map = state
        .repo
        .from_json(&json)
        .unwrap_or_else(|_| MindMap::new("Load Failed".into()));
    state.invoker.lock().unwrap().clear();
    let warning = match state.repo.store(map.clone()) {
        Ok(()) => None,
        Err(e) => Some(format!("自动保存失败: {e}")),
    };
    build_dto_from(&state, map, warning)
}

// =================================================================
//  节点级命令（走 CommandInvoker 以支持 undo/redo）
// =================================================================

/// 添加子节点。
#[tauri::command]
pub fn add_child_node(
    parent_id: Uuid,
    text: String,
    state: State<'_, crate::AppState>,
) -> DocumentDto {
    run_command(
        &state,
        add_child_command(parent_id, text),
        |_map, created| DomainEvent::NodeAdded {
            node_id: created.unwrap_or(parent_id),
            parent_id: Some(parent_id),
        },
    )
}

/// 添加游离节点（漂浮主题）：不挂在树上，布局时放置到主树外侧。
#[tauri::command]
pub fn add_free_node(
    text: String,
    state: State<'_, crate::AppState>,
) -> DocumentDto {
    run_command(
        &state,
        add_free_node_command(text.clone()),
        |_map, created| DomainEvent::NodeAdded {
            node_id: created.unwrap_or_else(|| uuid::Uuid::new_v4()),
            parent_id: None,
        },
    )
}

/// 添加兄弟节点。
#[tauri::command]
pub fn add_sibling_node(
    sibling_id: Uuid,
    text: String,
    state: State<'_, crate::AppState>,
) -> DocumentDto {
    run_command(
        &state,
        add_sibling_command(sibling_id, text),
        |_map, created| DomainEvent::NodeAdded {
            node_id: created.unwrap_or(sibling_id),
            parent_id: None,
        },
    )
}

/// 更新节点文本。
#[tauri::command]
pub fn update_node_text(
    node_id: Uuid,
    text: String,
    state: State<'_, crate::AppState>,
) -> DocumentDto {
    let old = state
        .repo
        .load()
        .ok()
        .and_then(|m| m.get_node(node_id).map(|n| n.text.clone()))
        .unwrap_or_default();
    run_command(
        &state,
        update_text_command(node_id, text.clone()),
        |_map, _created| DomainEvent::NodeTextUpdated {
            node_id,
            old,
            new: text,
        },
    )
}

/// 更新节点样式。
#[tauri::command]
pub fn update_node_style(
    node_id: Uuid,
    style: NodeStyle,
    state: State<'_, crate::AppState>,
) -> DocumentDto {
    run_command(
        &state,
        update_style_command(node_id, style),
        |_map, _created| DomainEvent::NodeStyleUpdated { node_id },
    )
}

/// 更新节点备注（Notes）。备注是附加的长文本，隐藏在主文本之后。
#[tauri::command]
pub fn update_node_notes(
    node_id: Uuid,
    notes: String,
    state: State<'_, crate::AppState>,
) -> DocumentDto {
    run_command(
        &state,
        update_notes_command(node_id, notes),
        |_map, _created| DomainEvent::NodeNotesUpdated { node_id },
    )
}

/// 更新文档标题（多文档管理：标题可编辑，可撤销）。
#[tauri::command]
pub fn update_document_title(
    title: String,
    state: State<'_, crate::AppState>,
) -> DocumentDto {
    run_command(
        &state,
        update_title_command(title.clone()),
        |_map, _created| DomainEvent::DocumentTitleUpdated { title },
    )
}

/// 移动节点（改变父节点）。
#[tauri::command]
pub fn move_node(
    node_id: Uuid,
    new_parent_id: Uuid,
    state: State<'_, crate::AppState>,
) -> DocumentDto {
    run_command(
        &state,
        move_node_command(node_id, new_parent_id),
        |_map, _created| DomainEvent::NodeMoved {
            node_id,
            new_parent_id,
        },
    )
}

/// 删除节点及其子树。
#[tauri::command]
pub fn remove_node(
    node_id: Uuid,
    state: State<'_, crate::AppState>,
) -> DocumentDto {
    run_command(
        &state,
        remove_node_command(node_id),
        |_map, _created| DomainEvent::NodeRemoved { node_id },
    )
}

/// 切换节点折叠状态。
#[tauri::command]
pub fn toggle_fold(
    node_id: Uuid,
    state: State<'_, crate::AppState>,
) -> DocumentDto {
    let folded = state
        .repo
        .load()
        .ok()
        .and_then(|m| m.get_node(node_id).map(|n| n.folded))
        .unwrap_or(false);
    run_command(
        &state,
        toggle_fold_command(node_id),
        |_map, _created| DomainEvent::NodeFoldToggled { node_id, folded },
    )
}

/// 新增关系线（跨节点自定义连接）。
#[tauri::command]
pub fn add_edge(
    source: Uuid,
    target: Uuid,
    state: State<'_, crate::AppState>,
) -> DocumentDto {
    run_command(
        &state,
        add_edge_command(source, target),
        |_map, created| DomainEvent::EdgeAdded {
            edge_id: created.unwrap_or_else(|| uuid::Uuid::new_v4()),
            source,
            target,
        },
    )
}

/// 删除关系线。
#[tauri::command]
pub fn remove_edge(
    edge_id: Uuid,
    state: State<'_, crate::AppState>,
) -> DocumentDto {
    run_command(
        &state,
        remove_edge_command(edge_id),
        |_map, _created| DomainEvent::EdgeRemoved { edge_id },
    )
}

/// 设置节点自由坐标（拖拽吸附终点持久化）。
///
/// 前端在节点拖拽无目标松开时调用，把终点写入 `Node::free_pos`，
/// 布局引擎重算时优先使用该坐标——拖拽位置不再被拉回算法布局。
#[tauri::command]
pub fn set_node_free_pos(
    node_id: Uuid,
    x: f32,
    y: f32,
    state: State<'_, crate::AppState>,
) -> DocumentDto {
    run_command(
        &state,
        set_free_pos_command(node_id, x, y),
        |_map, _created| DomainEvent::NodeFreePosSet { node_id },
    )
}

// =================================================================
//  历史：撤销 / 重做（真正调用共享 invoker）
// =================================================================

/// 撤销上一步操作。
#[tauri::command]
pub fn undo(state: State<'_, crate::AppState>) -> DocumentDto {
    let mut map = match state.repo.load() {
        Ok(m) => m,
        Err(e) => return build_dto(&state, Some(format!("读取文档失败: {e}"))),
    };
    {
        let mut inv = state.invoker.lock().unwrap();
        if !inv.undo(&mut map) {
            return snapshot(&state);
        }
    }
    let warning = match state.repo.store(map.clone()) {
        Ok(()) => None,
        Err(e) => Some(format!("自动保存失败: {e}")),
    };
    build_dto_from(&state, map, warning)
}

/// 重做最近一次被撤销的操作。
#[tauri::command]
pub fn redo(state: State<'_, crate::AppState>) -> DocumentDto {
    let mut map = match state.repo.load() {
        Ok(m) => m,
        Err(e) => return build_dto(&state, Some(format!("读取文档失败: {e}"))),
    };
    {
        let mut inv = state.invoker.lock().unwrap();
        if !inv.redo(&mut map) {
            return snapshot(&state);
        }
    }
    let warning = match state.repo.store(map.clone()) {
        Ok(()) => None,
        Err(e) => Some(format!("自动保存失败: {e}")),
    };
    build_dto_from(&state, map, warning)
}

// =================================================================
//  布局 / 导出
// =================================================================

/// 切换布局策略。
#[tauri::command]
pub fn set_layout_strategy(
    strategy: String,
    state: State<'_, crate::AppState>,
) -> DocumentDto {
    {
        let mut engine = state.layout.lock().unwrap();
        engine.set_strategy(&strategy);
    }
    snapshot(&state)
}

/// 导出文档为 Markdown / SVG / Outline / OPML。
#[tauri::command]
pub fn export_document(
    format: String,
    state: State<'_, crate::AppState>,
) -> String {
    let map = state
        .repo
        .load()
        .unwrap_or_else(|_| MindMap::new("Error".into()));
    exporter::export(&map, &format)
}

/// 从 OPML 字符串导入思维导图（替换当前文档）。清空历史。
///
/// OPML 是大纲类工具（Workflowy、幕布、Feedly）的事实交换标准，
/// 此命令让 VeinMind 能与生态互通。
#[tauri::command]
pub fn import_opml(
    opml: String,
    state: State<'_, crate::AppState>,
) -> Result<DocumentDto, String> {
    let map = exporter::import_opml(&opml)?;
    state.invoker.lock().unwrap().clear();
    let warning = match state.repo.store(map.clone()) {
        Ok(()) => None,
        Err(e) => Some(format!("自动保存失败: {e}")),
    };
    Ok(build_dto_from(&state, map, warning))
}

/// 从 FreeMind `.mm` 字符串导入思维导图（替换当前文档）。清空历史。
///
/// FreeMind 是 XMind / MindManager / 幕布 / ProcessOn 等共同支持的
/// 交换格式，此命令让 VeinMind 能与这些生态互通。
#[tauri::command]
pub fn import_freemind(
    mm: String,
    state: State<'_, crate::AppState>,
) -> Result<DocumentDto, String> {
    let map = exporter::import_freemind(&mm)?;
    state.invoker.lock().unwrap().clear();
    let warning = match state.repo.store(map.clone()) {
        Ok(()) => None,
        Err(e) => Some(format!("自动保存失败: {e}")),
    };
    Ok(build_dto_from(&state, map, warning))
}
