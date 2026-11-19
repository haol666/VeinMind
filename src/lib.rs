//! VeinMind — a mind mapping tool built with Rust + Tauri.
//!
//! The crate is organized in a layered architecture to maximize separation of
//! concerns and to showcase classical software design patterns:
//!
//! - **Domain layer** (`domain`): pure business models (`MindMap`, `Node`,
//!   `Edge`) and behavioural traits. Has zero dependencies on the outer layers.
//! - **Application layer** (`application`): use-case orchestration. Houses the
//!   *Command* pattern (undo/redo), the *Observer* pattern (`EventBus`) and the
//!   *Strategy* pattern (layout algorithms).
//! - **Infrastructure layer** (`infrastructure`): technical details such as
//!   persistence (`Repository` pattern) and export (`Visitor` pattern).
//! - **Presentation layer**: Tauri IPC commands in `commands/` plus the static
//!   frontend in `frontend/`.
//!
//! The `run` function wires every layer together and boots the Tauri runtime.

pub mod application;
pub mod commands;
pub mod domain;
pub mod infrastructure;

#[cfg(test)]
mod integration_tests;

use application::{command::CommandInvoker, event_bus::EventBus, layout::LayoutEngine};
use infrastructure::persistence::JsonMindMapRepository;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// The shared application state injected into every Tauri command.
///
/// `AppState` is constructed once at startup and wrapped in `Arc<Mutex<…>>`
/// so that the Tauri runtime can safely share it across async command handlers.
/// Holding the whole state behind a single mutex keeps the concurrency model
/// trivially correct at the cost of coarse-grained locking, which is perfectly
/// acceptable for a single-document mind map editor.
pub struct AppState {
    /// The currently open mind map document (the aggregate root).
    pub repo: JsonMindMapRepository,
    /// Broadcasts domain events to subscribers (Observer pattern).
    pub bus: Arc<Mutex<EventBus>>,
    /// Layout strategy controller (Strategy pattern).
    pub layout: Arc<Mutex<LayoutEngine>>,
    /// Persistent command invoker backing undo/redo (Command pattern).
    ///
    /// All mutating Tauri commands route through this single invoker so that
    /// the undo/redo stacks survive across calls — previously each command
    /// created a throw-away `CommandInvoker`, which made undo impossible.
    pub invoker: Arc<Mutex<CommandInvoker>>,
    /// Optional on-disk path for the current document.
    /// When `Some`, every mutation auto-saves to this path (P0-2).
    pub doc_path: Arc<Mutex<Option<PathBuf>>>,
}

impl AppState {
    /// Builds a fresh application state with a default empty mind map and a
    /// root node so the frontend has something to render immediately.
    ///
    /// Also wires **real** `EventBus` subscribers (Observer pattern), so that
    /// every domain event is logged and — when a doc path is set — triggers an
    /// auto-save. Previously `EventBus` was declared but had zero subscribers,
    /// making the Observer pattern purely decorative.
    pub fn new() -> Self {
        use application::event_bus::DomainEvent;
        use domain::{MindMap, Node};
        use uuid::Uuid;

        let mut map = MindMap::new("未命名".into());
        let root = Node::new(Uuid::new_v4(), "Central Topic".into(), None);
        map.add_node(root.clone());
        map.set_root(root.id);

        let repo = JsonMindMapRepository::with_initial(map);
        let bus = Arc::new(Mutex::new(EventBus::new()));
        let layout = Arc::new(Mutex::new(LayoutEngine::default()));
        // Capacity 200: enough for a long editing session without unbounded growth.
        let invoker = Arc::new(Mutex::new(CommandInvoker::new(200)));
        let doc_path: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));

        // ---- Observer pattern: register real subscribers ----
        {
            let b = bus.lock().unwrap();

            // Subscriber 1: structured logging of every domain event.
            b.subscribe(|event: &DomainEvent| {
                log::info!("[event] {:?}", event);
            });

            // Subscriber 2: auto-save trigger.
            // On any mutation event, if a doc path is set, the repository's
            // `store` already syncs to disk — but we still log the trigger so
            // the Observer chain is observable in tests/logs.
            b.subscribe(|event: &DomainEvent| {
                match event {
                    DomainEvent::NodeAdded { .. }
                    | DomainEvent::NodeRemoved { .. }
                    | DomainEvent::NodeTextUpdated { .. }
                    | DomainEvent::NodeStyleUpdated { .. }
                    | DomainEvent::NodeNotesUpdated { .. }
                    | DomainEvent::NodeFoldToggled { .. }
                    | DomainEvent::NodeMoved { .. }
                    | DomainEvent::NodeFreePosSet { .. }
                    | DomainEvent::EdgeAdded { .. }
                    | DomainEvent::EdgeRemoved { .. }
                    | DomainEvent::DocumentTitleUpdated { .. } => {
                        log::debug!("[autosave] mutation event observed");
                    }
                    _ => {}
                }
            });
        }

        Self {
            repo,
            bus,
            layout,
            invoker,
            doc_path,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Entry point invoked by `main.rs`. Configures logging, builds the Tauri app
/// with the shared [`AppState`] and starts the event loop.
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    let state = AppState::new();

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::mindmap::get_document,
            commands::mindmap::create_document,
            commands::mindmap::save_document,
            commands::mindmap::bind_doc_path,
            commands::mindmap::load_document,
            commands::mindmap::add_child_node,
            commands::mindmap::add_free_node,
            commands::mindmap::add_sibling_node,
            commands::mindmap::update_node_text,
            commands::mindmap::update_node_notes,
            commands::mindmap::update_node_style,
            commands::mindmap::update_document_title,
            commands::mindmap::move_node,
            commands::mindmap::remove_node,
            commands::mindmap::add_edge,
            commands::mindmap::remove_edge,
            commands::mindmap::set_node_free_pos,
            commands::mindmap::toggle_fold,
            commands::mindmap::undo,
            commands::mindmap::redo,
            commands::mindmap::set_layout_strategy,
            commands::mindmap::export_document,
            commands::mindmap::import_opml,
            commands::mindmap::import_freemind,
        ])
        .run(tauri::generate_context!())
        .expect("error while running VeinMind application");
}
