//! 仓储模式 (Repository Pattern) + JSON 持久化
//!
//! `MindMapRepository` trait 定义持久化抽象，`JsonMindMapRepository`
//! 为其具体实现（JSON 文件/内存）。切换为 SQLite 等后端只需新增一个
//! 实现，上层代码无感——符合 **依赖倒置原则**。
//!
//! 自动保存：当 `doc_path` 设置后，`store` 会同步落盘到该路径，
//! 保证关闭应用不丢数据（P0-2）。

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::domain::MindMap;

/// 自动保存去抖窗口：窗口内多次变更只落盘一次，避免每次按键都写全文件。
const AUTOSAVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// 持久化错误。
#[derive(Debug, Error)]
pub enum RepoError {
    #[error("document not found")]
    NotFound,
    #[error("serialization failed: {0}")]
    Serialization(String),
    #[error("I/O error: {0}")]
    Io(String),
    #[error("lock poisoned")]
    Poisoned,
}

/// 思维导图仓储 trait。
pub trait MindMapRepository: Send + Sync {
    /// 读取当前文档快照。
    fn load(&self) -> Result<MindMap, RepoError>;
    /// 写入新文档快照（整图替换）。若设置了 `doc_path`，同步落盘。
    fn store(&self, map: MindMap) -> Result<(), RepoError>;
    /// 序列化为 JSON 字符串（用于“另存为”）。
    fn to_json(&self) -> Result<String, RepoError>;
    /// 从 JSON 字符串反序列化并替换当前文档。
    fn from_json(&self, json: &str) -> Result<MindMap, RepoError>;
    /// 设置磁盘路径。之后的 `store` 会写到此文件。
    fn set_path(&self, path: PathBuf);
    /// 取当前磁盘路径。
    fn path(&self) -> Option<PathBuf>;
}

/// 仓储内部状态。把 `Mutex<MindMap>` 和可选路径打包在一起，
/// 避免 `AppState` 持有多个锁。
struct RepoInner {
    map: MindMap,
    path: Option<PathBuf>,
    /// 最近一次实际落盘时刻（用于自动保存去抖）。
    last_write: Option<Instant>,
    /// 有变更但尚未落盘（去抖窗口内被合并）。
    dirty: bool,
    /// 最近一次落盘错误（异步上报，供下一次 `store` 返回）。
    last_io_error: Option<String>,
}

/// 基于内存 + JSON 序列化的仓储实现。
///
/// 内部用 `Mutex` 保护单一文档，线程安全。
///
/// 自动保存采用 **去抖 + 原子写**：
///   - 去抖：`AUTOSAVE_DEBOUNCE` 窗口内多次 `store` 只写盘一次（内存始终即时更新，
///     磁盘按窗口合并），避免每次按键都做全量序列化 + 写盘；
///   - 原子写：先写同目录临时文件再 rename，避免中途崩溃留下半截损坏的 JSON；
///   - 兜底：窗口内未落盘的变更由 [`JsonMindMapRepository::flush`] 在退出前补写，
///     保证关闭应用不丢数据（P0-2）。
pub struct JsonMindMapRepository {
    inner: Mutex<RepoInner>,
}

impl JsonMindMapRepository {
    pub fn new() -> Self {
        let map = MindMap::new("未命名".into());
        Self {
            inner: Mutex::new(RepoInner {
                map,
                path: None,
                last_write: None,
                dirty: false,
                last_io_error: None,
            }),
        }
    }

    /// 以指定初始文档构造（供 `AppState::new` 使用）。
    pub fn with_initial(map: MindMap) -> Self {
        Self {
            inner: Mutex::new(RepoInner {
                map,
                path: None,
                last_write: None,
                dirty: false,
                last_io_error: None,
            }),
        }
    }

    /// 内部：加锁并返回 guard，避免重复错误处理样板。
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, RepoInner>, RepoError> {
        self.inner.lock().map_err(|_| RepoError::Poisoned)
    }

    /// 强制同步落盘（去抖窗口未到也立即写）。供退出前兜底与测试使用。
    pub fn flush(&self) -> Result<(), RepoError> {
        let mut g = self.lock()?;
        let path = match &g.path {
            Some(p) => p.clone(),
            None => return Ok(()),
        };
        let json = serde_json::to_string_pretty(&g.map)
            .map_err(|e| RepoError::Serialization(e.to_string()))?;
        write_file_atomic(&path, &json)?;
        g.last_write = Some(Instant::now());
        g.dirty = false;
        Ok(())
    }
}

impl Default for JsonMindMapRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl MindMapRepository for JsonMindMapRepository {
    fn load(&self) -> Result<MindMap, RepoError> {
        Ok(self.lock()?.map.clone())
    }

    fn store(&self, map: MindMap) -> Result<(), RepoError> {
        let mut g = self.lock()?;
        // 先更新内存（始终即时，读一致性优先）
        g.map = map;
        // 把上一次落盘错误转交本次调用（若有），保证写盘失败最终可见
        let prev_err = g.last_io_error.take();
        // 若设置了磁盘路径，按去抖窗口决定是否落盘
        if g.path.is_some() {
            let due = match g.last_write {
                Some(t) => t.elapsed() >= AUTOSAVE_DEBOUNCE,
                None => true,
            };
            if due {
                match flush_locked(&mut g) {
                    Ok(()) => {}
                    Err(e) => {
                        g.last_io_error = Some(e.to_string());
                    }
                }
            } else {
                // 窗口内：合并到下一次落盘（dirty 标记）
                g.dirty = true;
            }
        }
        match prev_err {
            Some(e) => Err(RepoError::Io(e)),
            None => Ok(()),
        }
    }

    fn to_json(&self) -> Result<String, RepoError> {
        let g = self.lock()?;
        serde_json::to_string_pretty(&g.map).map_err(|e| RepoError::Serialization(e.to_string()))
    }

    fn from_json(&self, json: &str) -> Result<MindMap, RepoError> {
        let mut map: MindMap = serde_json::from_str(json)
            .map_err(|e| RepoError::Serialization(e.to_string()))?;
        // node_index 不参与序列化，反序列化后重建
        map.rebuild_index();
        self.store(map.clone())?;
        Ok(map)
    }

    fn set_path(&self, path: PathBuf) {
        if let Ok(mut g) = self.lock() {
            g.path = Some(path);
        }
    }

    fn path(&self) -> Option<PathBuf> {
        self.lock().ok().and_then(|g| g.path.clone())
    }
}

/// 在持有锁的情况下执行原子落盘（临时文件 + rename）。
fn flush_locked(g: &mut RepoInner) -> Result<(), RepoError> {
    let path = match &g.path {
        Some(p) => p.clone(),
        None => return Ok(()),
    };
    let json = serde_json::to_string_pretty(&g.map)
        .map_err(|e| RepoError::Serialization(e.to_string()))?;
    write_file_atomic(&path, &json)?;
    g.last_write = Some(Instant::now());
    g.dirty = false;
    Ok(())
}

/// 原子写文件：先写同目录临时文件，再 rename 覆盖目标，防止崩溃留下半截文件。
fn write_file_atomic(path: &Path, contents: &str) -> Result<(), RepoError> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, contents).map_err(|e| RepoError::Io(e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| RepoError::Io(e.to_string()))?;
    Ok(())
}
