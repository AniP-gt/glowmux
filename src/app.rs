use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::ai_title;
use crate::config::{ConfigFile, FeaturesConfig};
use crate::filetree::FileTree;
use crate::hooks::HookEvent;
use crate::pane::Pane;
use crate::preview::Preview;

/// Events dispatched within the app.
#[allow(dead_code)]
pub enum AppEvent {
    /// PTY output received for a pane. `lines` contains new printable lines from the PTY.
    PtyOutput { pane_id: usize, lines: Vec<String> },
    /// PTY process exited for a pane.
    PtyEof(usize),
    /// Shell changed working directory (pane_id, new path).
    CwdChanged(usize, PathBuf),
    /// Hook event received from Unix socket server.
    HookReceived { pane_id: usize, event: HookEvent },
    /// AI title generation completed (None = generation failed/timed out).
    AiTitleGenerated { pane_id: usize, title: Option<String> },
    /// AI branch name generation completed.
    BranchNameGenerated { branch: String },
    /// Async worktree creation completed successfully.
    WorktreeCreated { pane_id: usize, cwd: std::path::PathBuf, branch_name: String },
    /// Async worktree creation failed.
    WorktreeCreateFailed { pane_id: usize, branch_name: String, error: String },
    /// Worktree branch has been merged into main.
    WorktreeMerged { worktree_path: std::path::PathBuf },
    /// Worktree list refresh completed.
    WorktreesListed { worktrees: Vec<crate::worktree::WorktreeInfo> },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PaneStatus {
    Idle,
    Running,
    Done,
    Waiting,
}

#[derive(Debug, Clone)]
pub struct PaneState {
    pub status: PaneStatus,
    pub dismissed: bool,
}

impl Default for PaneState {
    fn default() -> Self {
        Self {
            status: PaneStatus::Idle,
            dismissed: false,
        }
    }
}

pub const FEATURES: &[(&str, &str)] = &[
    ("status_dot", "Status Dot"),
    ("status_bg_color", "BG Color"),
    ("status_bar", "Status Bar"),
    ("worktree", "Worktree"),
    ("worktree_ai_name", "Worktree AI Name"),
    ("file_tree", "File Tree"),
    ("file_preview", "File Preview"),
    ("diff_preview", "Diff Preview"),
    ("cd_tracking", "CD Tracking"),
    ("ai_title", "AI Title"),
    ("responsive_layout", "Responsive Layout"),
    ("session_persist", "Session Persist"),
    ("context_copy", "Context Copy"),
    ("layout_picker", "Layout Picker"),
    ("startup_panes", "Startup Panes"),
    ("zoom", "Zoom"),
];

#[derive(Debug, Clone)]
pub struct FeatureToggleState {
    pub visible: bool,
    pub selected: usize,
    pub pending: FeaturesConfig,
}

pub const SETTINGS_ITEMS: &[(&str, &str)] = &[
    ("terminal.scrollback", "Scrollback Lines"),
    ("layout.breakpoint_stack", "Layout Breakpoint (stack)"),
    ("layout.breakpoint_split2", "Layout Breakpoint (split2)"),
    ("ai_title_engine.update_interval_sec", "AI Title Update Interval (sec)"),
    ("ai_title_engine.max_chars", "AI Title Max Chars"),
];

#[derive(Debug, Clone)]
pub struct SettingsPanelState {
    pub visible: bool,
    pub selected: usize,
    pub editing: bool,
    pub edit_buffer: String,
}

/// Split direction for layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplitDirection {
    Vertical,
    Horizontal,
}

/// Which area has focus.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusTarget {
    Pane,
    FileTree,
    Preview,
}

/// Layout mode for the workspace.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayoutMode {
    Stack,
    TwoSplit,
    Grid,
    MainSub,
    BigOnePlusThree,
    Auto,
}

#[derive(Debug, Clone, Default)]
pub struct LayoutPickerState {
    pub visible: bool,
    pub selected: usize,
}

/// Direction for pane focus movement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PaneCreateField {
    BranchName,
    BaseBranch,
    WorktreeToggle,
    AgentField,
    AiGenerate,
    OkButton,
    CancelButton,
}

#[derive(Debug, Clone)]
pub struct PaneCreateDialog {
    pub visible: bool,
    pub branch_name: String,
    pub base_branch: String,
    pub worktree_enabled: bool,
    pub agent: String,
    pub generating_name: bool,
    pub focused_field: PaneCreateField,
    pub error_msg: Option<String>,
}

impl Default for PaneCreateDialog {
    fn default() -> Self {
        Self {
            visible: false,
            branch_name: String::new(),
            base_branch: String::new(),
            worktree_enabled: false,
            agent: String::new(),
            generating_name: false,
            focused_field: PaneCreateField::BranchName,
            error_msg: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CloseConfirmFocus {
    Yes,
    No,
}

#[derive(Debug, Clone)]
pub struct CloseConfirmDialog {
    pub visible: bool,
    pub pane_id: usize,
    pub worktree_path: Option<std::path::PathBuf>,
    pub focused: CloseConfirmFocus,
}

impl Default for CloseConfirmDialog {
    fn default() -> Self {
        Self {
            visible: false,
            pane_id: 0,
            worktree_path: None,
            focused: CloseConfirmFocus::No,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorktreeCleanupDialog {
    pub visible: bool,
    pub worktree_path: std::path::PathBuf,
    pub branch: String,
    pub focused: CloseConfirmFocus,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CopyModeAction {
    Continue,
    Quit,
    Yank,
}

#[derive(Debug, Clone, Default)]
pub struct CopyModeState {
    pub pane_id: usize,
    pub cursor_row: u16,
    pub cursor_col: u16,
    pub selection_start: Option<(u16, u16)>,
    pub line_wise: bool,
    pub screen_rows: u16,
    pub screen_cols: u16,
    pub first_g: bool,
    pub scrollback_offset: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PaneListOverlay {
    pub visible: bool,
    pub selected: usize,
    pub pane_ids: Vec<usize>,
}

const FILETREE_ACTION_COUNT: usize = 2;

#[derive(Debug, Clone, Default)]
pub struct FileTreeActionPopup {
    pub visible: bool,
    pub file_path: std::path::PathBuf,
    pub selected: usize,
}

/// Which border is being dragged.
#[derive(Debug, Clone, PartialEq)]
pub enum DragTarget {
    FileTreeBorder,
    PreviewBorder,
    PaneSplit(Vec<bool>, SplitDirection, Rect),
    Scrollbar(usize, Rect), // pane_id, inner area
}


/// Binary tree node for pane layout.
pub enum LayoutNode {
    Leaf { pane_id: usize },
    Split {
        direction: SplitDirection,
        ratio: f32, // 0.0..1.0, portion allocated to first child
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

impl LayoutNode {
    pub fn collect_pane_ids(&self) -> Vec<usize> {
        match self {
            LayoutNode::Leaf { pane_id } => vec![*pane_id],
            LayoutNode::Split { first, second, .. } => {
                let mut ids = first.collect_pane_ids();
                ids.extend(second.collect_pane_ids());
                ids
            }
        }
    }

    pub fn calculate_rects(&self, area: Rect) -> Vec<(usize, Rect)> {
        match self {
            LayoutNode::Leaf { pane_id } => vec![(*pane_id, area)],
            LayoutNode::Split { direction, ratio, first, second } => {
                let (first_area, second_area) = split_rect(area, *direction, *ratio);
                let mut result = first.calculate_rects(first_area);
                result.extend(second.calculate_rects(second_area));
                result
            }
        }
    }

    pub fn split_pane(&mut self, target_id: usize, new_id: usize, direction: SplitDirection) -> bool {
        match self {
            LayoutNode::Leaf { pane_id } => {
                if *pane_id == target_id {
                    let old_id = *pane_id;
                    *self = LayoutNode::Split {
                        direction,
                        ratio: 0.5,
                        first: Box::new(LayoutNode::Leaf { pane_id: old_id }),
                        second: Box::new(LayoutNode::Leaf { pane_id: new_id }),
                    };
                    true
                } else {
                    false
                }
            }
            LayoutNode::Split { first, second, .. } => {
                first.split_pane(target_id, new_id, direction)
                    || second.split_pane(target_id, new_id, direction)
            }
        }
    }

    pub fn remove_pane(&mut self, target_id: usize) -> bool {
        match self {
            LayoutNode::Leaf { .. } => false,
            LayoutNode::Split { first, second, .. } => {
                if let LayoutNode::Leaf { pane_id } = first.as_ref() {
                    if *pane_id == target_id {
                        let second = std::mem::replace(second.as_mut(), LayoutNode::Leaf { pane_id: 0 });
                        *self = second;
                        return true;
                    }
                }
                if let LayoutNode::Leaf { pane_id } = second.as_ref() {
                    if *pane_id == target_id {
                        let first = std::mem::replace(first.as_mut(), LayoutNode::Leaf { pane_id: 0 });
                        *self = first;
                        return true;
                    }
                }
                first.remove_pane(target_id) || second.remove_pane(target_id)
            }
        }
    }

    /// Find the split boundary position and direction for hit testing.
    /// Returns a list of (boundary_position, direction, depth) for each Split node.
    pub fn split_boundaries(&self, area: Rect) -> Vec<(u16, SplitDirection, Vec<bool>)> {
        let mut result = Vec::new();
        self.collect_boundaries(area, &mut Vec::new(), &mut result);
        result
    }

    fn collect_boundaries(
        &self,
        area: Rect,
        path: &mut Vec<bool>, // false=first, true=second
        result: &mut Vec<(u16, SplitDirection, Vec<bool>)>,
    ) {
        if let LayoutNode::Split { direction, ratio, first, second } = self {
            let (first_area, second_area) = split_rect(area, *direction, *ratio);

            // The boundary is at the edge between first and second
            let boundary = match direction {
                SplitDirection::Vertical => first_area.x + first_area.width,
                SplitDirection::Horizontal => first_area.y + first_area.height,
            };
            result.push((boundary, *direction, path.clone()));

            path.push(false);
            first.collect_boundaries(first_area, path, result);
            path.pop();

            path.push(true);
            second.collect_boundaries(second_area, path, result);
            path.pop();
        }
    }

    /// Update ratio by path (path identifies which Split node).
    pub fn update_ratio(&mut self, path: &[bool], new_ratio: f32) {
        if path.is_empty() {
            if let LayoutNode::Split { ratio, .. } = self {
                *ratio = new_ratio.clamp(0.15, 0.85);
            }
        } else if let LayoutNode::Split { first, second, .. } = self {
            if path[0] {
                second.update_ratio(&path[1..], new_ratio);
            } else {
                first.update_ratio(&path[1..], new_ratio);
            }
        }
    }

    pub fn clone_layout(&self) -> LayoutNode {
        match self {
            LayoutNode::Leaf { pane_id } => LayoutNode::Leaf { pane_id: *pane_id },
            LayoutNode::Split { direction, ratio, first, second } => LayoutNode::Split {
                direction: *direction,
                ratio: *ratio,
                first: Box::new(first.clone_layout()),
                second: Box::new(second.clone_layout()),
            },
        }
    }

    pub fn pane_count(&self) -> usize {
        match self {
            LayoutNode::Leaf { .. } => 1,
            LayoutNode::Split { first, second, .. } => first.pane_count() + second.pane_count(),
        }
    }
}

fn split_rect(area: Rect, direction: SplitDirection, ratio: f32) -> (Rect, Rect) {
    let ratio = ratio.clamp(0.1, 0.9);
    match direction {
        SplitDirection::Vertical => {
            let first_w = (area.width as f32 * ratio) as u16;
            let first_w = first_w.max(1).min(area.width.saturating_sub(1));
            (
                Rect::new(area.x, area.y, first_w, area.height),
                Rect::new(area.x + first_w, area.y, area.width - first_w, area.height),
            )
        }
        SplitDirection::Horizontal => {
            let first_h = (area.height as f32 * ratio) as u16;
            let first_h = first_h.max(1).min(area.height.saturating_sub(1));
            (
                Rect::new(area.x, area.y, area.width, first_h),
                Rect::new(area.x, area.y + first_h, area.width, area.height - first_h),
            )
        }
    }
}


/// What the current text selection is anchored to.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionTarget {
    Pane(usize),
    Preview,
}

/// Text selection state. Works for both terminal panes and the file
/// preview panel — `target` tells rendering and extraction which
/// source to read.
///
/// Coordinate semantics differ by target:
/// - **Pane**: start/end rows+cols are screen-relative to
///   `content_rect` (the inner area of the pane border).
/// - **Preview**: rows are **absolute line indices** into
///   `preview.lines`; cols are **char offsets** within the line.
///   This lets the selection survive vertical and horizontal
///   scrolling — overlay rendering subtracts the current scroll
///   to turn source coords back into screen coords.
#[derive(Debug, Clone)]
pub struct TextSelection {
    pub target: SelectionTarget,
    pub start_row: u32,
    pub start_col: u32,
    pub end_row: u32,
    pub end_col: u32,
    /// Content area used for coordinate mapping — the inside of the
    /// pane border, or (for previews) the area excluding the line
    /// number gutter.
    pub content_rect: Rect,
}

impl TextSelection {
    /// Get normalized (top-left to bottom-right) selection range.
    pub fn normalized(&self) -> (u32, u32, u32, u32) {
        if self.start_row < self.end_row
            || (self.start_row == self.end_row && self.start_col <= self.end_col)
        {
            (self.start_row, self.start_col, self.end_row, self.end_col)
        } else {
            (self.end_row, self.end_col, self.start_row, self.start_col)
        }
    }

    /// Check if a cell is within the selection.
    pub fn contains(&self, row: u32, col: u32) -> bool {
        let (sr, sc, er, ec) = self.normalized();
        if row < sr || row > er {
            return false;
        }
        if row == sr && row == er {
            return col >= sc && col <= ec;
        }
        if row == sr {
            return col >= sc;
        }
        if row == er {
            return col <= ec;
        }
        true
    }
}


/// A workspace holds all state for one tab.
#[allow(dead_code)]
pub struct Workspace {
    pub name: String,
    /// Session-only rename; when Some, takes precedence over `name` for
    /// display. Not persisted; `cd` does not touch this.
    pub custom_name: Option<String>,
    pub cwd: PathBuf,
    pub panes: HashMap<usize, Pane>,
    pub layout: LayoutNode,
    pub focused_pane_id: usize,
    pub file_tree: FileTree,
    pub file_tree_visible: bool,
    pub preview: Preview,
    pub focus_target: FocusTarget,
    // Cached rects (updated on each render)
    pub last_pane_rects: Vec<(usize, Rect)>,
    pub last_file_tree_rect: Option<Rect>,
    pub last_preview_rect: Option<Rect>,
    pub layout_mode: LayoutMode,
    pub worktrees: Vec<crate::worktree::WorktreeInfo>,
}

impl Workspace {
    fn new(
        name: String,
        cwd: PathBuf,
        pane_id: usize,
        rows: u16,
        cols: u16,
        event_tx: Sender<AppEvent>,
    ) -> Result<Self> {
        let pane = Pane::new(pane_id, rows, cols, event_tx)?;
        let mut panes = HashMap::new();
        panes.insert(pane_id, pane);

        Ok(Self {
            name,
            custom_name: None,
            file_tree: FileTree::new(cwd.clone()),
            cwd,
            panes,
            layout: LayoutNode::Leaf { pane_id },
            focused_pane_id: pane_id,
            file_tree_visible: true,
            preview: Preview::new(),
            focus_target: FocusTarget::Pane,
            last_pane_rects: Vec::new(),
            last_file_tree_rect: None,
            last_preview_rect: None,
            layout_mode: LayoutMode::Auto,
            worktrees: Vec::new(),
        })
    }

    fn shutdown(&mut self) {
        for pane in self.panes.values_mut() {
            pane.kill();
        }
    }

    /// Tab label to show in the UI: custom rename wins over the
    /// cwd-derived name.
    pub fn display_name(&self) -> &str {
        self.custom_name.as_deref().unwrap_or(&self.name)
    }
}


pub struct App {
    pub workspaces: Vec<Workspace>,
    pub active_tab: usize,
    pub should_quit: bool,
    pub event_tx: Sender<AppEvent>,
    pub event_rx: Receiver<AppEvent>,
    next_pane_id: usize,
    pub dirty: bool,
    pub paste_cooldown: u8, // frames to skip rendering after paste
    /// Frames to skip rendering after a layout change (split, close,
    /// sidebar toggle, terminal resize). Gives Claude Code / bash time
    /// to process SIGWINCH and send a fresh redraw before we paint,
    /// avoiding the brief "old buffer at new size" garbled frame.
    pub resize_cooldown: u8,
    /// Last known terminal size (cols, rows). Updated from main.rs on
    /// Event::Resize and from ui::render on every frame. Used by
    /// `relayout_panes()` so layout-change handlers can resize PTYs
    /// without needing a Frame reference.
    pub last_term_size: (u16, u16),
    // Shared settings
    pub file_tree_width: u16,
    pub preview_width: u16,
    // Layout: swap preview and terminal positions
    pub layout_swapped: bool,
    // Toggle status bar visibility (Alt+S)
    pub status_bar_visible: bool,
    // Drag/hover state
    pub dragging: Option<DragTarget>,
    pub hover_border: Option<DragTarget>,
    // Tab bar rects for mouse click
    pub last_tab_rects: Vec<(usize, Rect)>,
    pub last_new_tab_rect: Option<Rect>,
    /// Active tab rename input buffer. When `Some`, key input is
    /// routed to this buffer instead of the focused PTY; Enter commits
    /// to the active workspace's `custom_name`, Esc cancels.
    pub rename_input: Option<String>,
    /// (tab index, timestamp) of the last left-click on a tab label.
    /// Used to detect a double-click → enter rename mode.
    last_tab_click: Option<(usize, Instant)>,
    // Text selection
    pub selection: Option<TextSelection>,
    // Version check (background)
    pub version_info: crate::version_check::VersionInfo,
    // Claude Code JSONL monitoring
    pub claude_monitor: crate::claude_monitor::ClaudeMonitor,
    // Reusable clipboard handle (lazy-initialized)
    clipboard: Option<arboard::Clipboard>,
    // Image preview protocol picker
    pub image_picker: Option<ratatui_image::picker::Picker>,
    pub config: ConfigFile,
    pub zoomed_pane_id: Option<usize>,
    pub pre_zoom_layout: Option<LayoutNode>,
    pub layout_picker: LayoutPickerState,
    pub pane_states: HashMap<usize, PaneState>,
    pub ai_title_enabled: bool,
    pub feature_toggle: FeatureToggleState,
    pub settings_panel: SettingsPanelState,
    pub status_flash: Option<(String, std::time::Instant)>,
    pub pane_output_rings: HashMap<usize, VecDeque<String>>,
    pub last_ai_title_request: HashMap<usize, Instant>,
    pub ai_title_in_flight: std::collections::HashSet<usize>,
    pub ai_titles: HashMap<usize, String>,
    pub tokio_handle: Option<tokio::runtime::Handle>,
    pub pane_create_dialog: PaneCreateDialog,
    pub close_confirm_dialog: CloseConfirmDialog,
    pub worktree_cleanup_dialog: Option<WorktreeCleanupDialog>,
    pub prefix_active: bool,
    pub copy_mode: Option<CopyModeState>,
    pub pane_list_overlay: PaneListOverlay,
    pub filetree_action_popup: FileTreeActionPopup,
    /// When true, the preview panel is expanded to fill the full content area.
    pub preview_zoomed: bool,
}

impl App {
    pub fn new(rows: u16, cols: u16, config: ConfigFile) -> Result<Self> {
        let kb_warnings = crate::keybinding::validate_keybindings(&config.keybindings);
        for w in kb_warnings {
            eprintln!("glowmux: duplicate keybinding: {}", w);
        }

        let (event_tx, event_rx) = mpsc::channel();

        let pane_rows = rows.saturating_sub(5); // title + tab bar + status + borders
        let pane_cols = cols.saturating_sub(2);

        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let name = dir_name(&cwd);

        let ws = Workspace::new(name, cwd, 1, pane_rows, pane_cols, event_tx.clone())?;

        let ai_title_enabled_init = config.features.ai_title;
        let worktree_auto_create = config.worktree.auto_create;

        let mut app = Self {
            workspaces: vec![ws],
            active_tab: 0,
            should_quit: false,
            event_tx,
            event_rx,
            next_pane_id: 2,
            dirty: true,
            paste_cooldown: 0,
            resize_cooldown: 0,
            last_term_size: (cols, rows),
            file_tree_width: config.layout.file_tree_width,
            preview_width: config.layout.preview_width,
            layout_swapped: true,
            status_bar_visible: true,
            dragging: None,
            hover_border: None,
            last_tab_rects: Vec::new(),
            last_new_tab_rect: None,
            rename_input: None,
            last_tab_click: None,
            selection: None,
            version_info: {
                let info = crate::version_check::VersionInfo::new();
                crate::version_check::spawn_check(info.clone());
                info
            },
            claude_monitor: crate::claude_monitor::ClaudeMonitor::new(),
            clipboard: None,
            image_picker: None,
            config,
            zoomed_pane_id: None,
            pre_zoom_layout: None,
            layout_picker: LayoutPickerState::default(),
            pane_states: HashMap::new(),
            ai_title_enabled: ai_title_enabled_init,
            feature_toggle: FeatureToggleState {
                visible: false,
                selected: 0,
                pending: FeaturesConfig::default(),
            },
            settings_panel: SettingsPanelState {
                visible: false,
                selected: 0,
                editing: false,
                edit_buffer: String::new(),
            },
            status_flash: None,
            pane_output_rings: HashMap::new(),
            last_ai_title_request: HashMap::new(),
            ai_title_in_flight: std::collections::HashSet::new(),
            ai_titles: HashMap::new(),
            tokio_handle: None,
            pane_create_dialog: PaneCreateDialog {
                worktree_enabled: worktree_auto_create,
                ..Default::default()
            },
            close_confirm_dialog: CloseConfirmDialog::default(),
            worktree_cleanup_dialog: None,
            prefix_active: false,
            copy_mode: None,
            pane_list_overlay: PaneListOverlay::default(),
            filetree_action_popup: FileTreeActionPopup::default(),
            preview_zoomed: false,
        };

        // Session restore takes priority over startup panes. Only apply startup
        // panes when no saved session is loaded.
        let mut session_restored = false;
        if app.config.session.enabled && app.config.session.restore_on_start {
            if let Some(session_path) = crate::session::SessionData::session_path() {
                if session_path.exists() {
                    if let Some(session) = crate::session::SessionData::load(&session_path) {
                        if !session.workspaces.is_empty() {
                            let tx = app.event_tx.clone();
                            if restore_session_workspaces(
                                &mut app, &session, pane_rows, pane_cols, &tx,
                            ).is_ok() {
                                session_restored = true;
                            }
                        }
                    }
                }
            }
        }

        if !session_restored
            && app.config.features.startup_panes
            && app.config.startup.enabled
            && !app.config.startup.panes.is_empty()
        {
            app.apply_startup_panes(pane_rows, pane_cols)?;
        }

        Ok(app)
    }

    /// Copy text to clipboard, reusing the handle if available.
    fn copy_to_clipboard(&mut self, text: &str) {
        if self.clipboard.is_none() {
            self.clipboard = arboard::Clipboard::new().ok();
        }
        if let Some(ref mut cb) = self.clipboard {
            let _ = cb.set_text(text);
        }
    }

    /// Drop the current selection if it targets the preview. Called
    /// whenever preview state shifts (scroll, new file) so the
    /// highlighted range can't point at different text than what
    /// Ctrl+C or mouse-up actually copies.
    fn clear_selection_if_preview(&mut self) {
        if matches!(
            self.selection.as_ref().map(|s| &s.target),
            Some(SelectionTarget::Preview)
        ) {
            self.selection = None;
        }
    }

    /// Recompute pane rectangles and apply sizes to every PTY in the
    /// active workspace. Returns `true` if any pane was actually
    /// resized (so callers can decide whether to enter the post-resize
    /// cooldown). Safe to call without a Frame — uses the cached
    /// `last_term_size`.
    pub fn relayout_panes(&mut self) -> bool {
        let (cols, rows) = self.last_term_size;
        if cols < 20 || rows < 5 {
            return false;
        }

        // Mirror the area math in ui::render / render_main_area,
        // including the fallback where tree / preview are hidden when
        // the terminal is too narrow. Keeping these in sync prevents
        // PTY size drift from the actually-painted pane size.
        const MIN_PANE_AREA_WIDTH: u16 = 20;
        let tab_h = 1u16;
        let status_h: u16 = if self.status_bar_visible || self.rename_input.is_some() { 1 } else { 0 };
        let main_h = rows.saturating_sub(tab_h + status_h);

        let mut has_tree = self.ws().file_tree_visible;
        let mut has_preview = self.ws().preview.is_active();
        let tree_w_nom = self.file_tree_width;
        let preview_w_nom = self.preview_width;

        let needed = MIN_PANE_AREA_WIDTH
            + if has_tree { tree_w_nom } else { 0 }
            + if has_preview { preview_w_nom } else { 0 };
        if cols < needed && has_preview {
            has_preview = false;
        }
        let needed = MIN_PANE_AREA_WIDTH + if has_tree { tree_w_nom } else { 0 };
        if cols < needed && has_tree {
            has_tree = false;
        }

        let tree_w = if has_tree { tree_w_nom } else { 0 };
        let preview_w = if has_preview { preview_w_nom } else { 0 };
        let pane_w = cols.saturating_sub(tree_w).saturating_sub(preview_w);

        // x/y exact values don't matter for calculate_rects' sub-areas,
        // only width/height propagate into the recursive split sizes.
        let pane_area = Rect::new(0, tab_h, pane_w, main_h);
        let rects = self.ws().layout.calculate_rects(pane_area);

        let mut any_changed = false;
        for (pane_id, rect) in &rects {
            if let Some(pane) = self.ws_mut().panes.get_mut(pane_id) {
                let inner_rows = rect.height.saturating_sub(2);
                let inner_cols = rect.width.saturating_sub(2);
                if pane.resize(inner_rows, inner_cols).unwrap_or(false) {
                    any_changed = true;
                }
            }
        }

        self.ws_mut().last_pane_rects = rects;
        any_changed
    }

    /// Mark a layout change: apply resizes immediately and, if sizes
    /// actually changed, delay the next paint for a few frames so the
    /// PTY child can respond to SIGWINCH with a fresh redraw before
    /// we render. When no size changes happen (e.g. a sidebar toggle
    /// that fits in the same remaining width) we skip the cooldown so
    /// the UI stays responsive. Also drops any live selection, whose
    /// stored `content_rect` / `pane_id` could reference a layout that
    /// no longer exists.
    pub fn mark_layout_change(&mut self) {
        let changed = self.relayout_panes();
        if changed {
            // Take max so a freshly-triggered layout change on top of
            // an existing cooldown doesn't prematurely cut the wait.
            self.resize_cooldown = self.resize_cooldown.max(5);
        }
        // Any in-flight selection is bound to the old geometry.
        self.selection = None;
        self.dirty = true;
    }

    /// Called from main.rs on crossterm Resize events so we can update
    /// the cached terminal size and propagate the resize into panes.
    pub fn on_terminal_resize(&mut self, cols: u16, rows: u16) {
        self.last_term_size = (cols, rows);
        self.mark_layout_change();
    }

    /// Get the active workspace.
    pub fn ws(&self) -> &Workspace {
        &self.workspaces[self.active_tab]
    }

    /// Get the active workspace mutably.
    pub fn ws_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.active_tab]
    }

    fn key_matches(&self, key: crossterm::event::KeyEvent, binding: &str) -> bool {
        crate::keybinding::parse_keybinding(binding)
            .map(|(m, c)| key.modifiers == m && key.code == c)
            .unwrap_or(false)
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        // Rename mode — swallow all input until Enter/Esc.
        if self.rename_input.is_some() {
            return Ok(self.handle_rename_key(key));
        }

        // Settings panel dialog
        if self.settings_panel.visible {
            return self.handle_settings_panel_key(key);
        }

        // Feature toggle dialog
        if self.feature_toggle.visible {
            return self.handle_feature_toggle_key(key);
        }

        // Layout picker mode
        if self.layout_picker.visible {
            return self.handle_layout_picker_key(key);
        }

        // Pane create dialog
        if self.pane_create_dialog.visible {
            return self.handle_pane_create_key(key);
        }
        // Close confirm dialog
        if self.close_confirm_dialog.visible {
            return self.handle_close_confirm_key(key);
        }
        // Worktree cleanup dialog
        if self.worktree_cleanup_dialog.as_ref().is_some_and(|d| d.visible) {
            return self.handle_worktree_cleanup_key(key);
        }

        // Copy mode modal
        if self.copy_mode.is_some() {
            return self.handle_copy_mode_key(key);
        }

        // Pane list overlay modal
        if self.pane_list_overlay.visible {
            return self.handle_pane_list_key(key);
        }

        // File tree action popup modal
        if self.filetree_action_popup.visible {
            return self.handle_filetree_action_popup_key(key);
        }

        // Prefix key handling
        let prefix_key = crate::keybinding::parse_keybinding(&self.config.keybindings.prefix);
        if let Some((prefix_mods, prefix_code)) = prefix_key {
            if !self.prefix_active {
                if key.modifiers == prefix_mods && key.code == prefix_code {
                    self.prefix_active = true;
                    self.dirty = true;
                    return Ok(true);
                }
            } else {
                self.prefix_active = false;
                self.dirty = true;
                if key.modifiers == prefix_mods && key.code == prefix_code {
                    // Prefix pressed twice: fall through to PTY passthrough
                } else if crate::keybinding::parse_keybinding(&self.config.keybindings.quit)
                    .map(|(_, code)| key.code == code)
                    .unwrap_or(false)
                {
                    self.should_quit = true;
                    return Ok(true);
                } else if crate::keybinding::parse_keybinding(&self.config.keybindings.layout_cycle)
                    .map(|(_, code)| key.code == code)
                    .unwrap_or(false)
                {
                    // Prefix + layout_cycle key — cycle layout mode
                    self.cycle_layout_mode();
                    return Ok(true);
                } else if key.code == KeyCode::Char('[') {
                    self.enter_copy_mode();
                    return Ok(true);
                } else if key.code == KeyCode::Char('w') {
                    self.open_pane_list_overlay();
                    return Ok(true);
                }
            }
        }

        // Quit (configurable, default Ctrl+Q)
        if self.key_matches(key, &self.config.keybindings.quit) {
            self.should_quit = true;
            return Ok(true);
        }

        // Tab rename (configurable, default Alt+R)
        if self.key_matches(key, &self.config.keybindings.tab_rename) {
            self.rename_input = Some(String::new());
            if !self.status_bar_visible {
                self.mark_layout_change();
            }
            return Ok(true);
        }

        // Ctrl+C — if text is selected, copy to clipboard instead of sending SIGINT
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
            if let Some(ref sel) = self.selection.clone() {
                let (sr, sc, er, ec) = sel.normalized();
                if sr != er || sc != ec {
                    let text = match sel.target {
                        SelectionTarget::Pane(pane_id) => self
                            .ws()
                            .panes
                            .get(&pane_id)
                            .map(|p| extract_selected_text(p, sr, sc, er, ec))
                            .unwrap_or_default(),
                        SelectionTarget::Preview => extract_preview_selected_text(
                            &self.ws().preview,
                            sr,
                            sc,
                            er,
                            ec,
                        ),
                    };
                    if !text.is_empty() {
                        self.copy_to_clipboard(&text);
                    }
                    self.selection = None;
                    return Ok(true);
                }
            }
            // No selection — fall through to forward Ctrl+C to PTY
        }

        // Clipboard copy — focused pane's visible content (configurable, default Ctrl+Y)
        if self.key_matches(key, &self.config.keybindings.clipboard_copy) {
            let content = {
                let pane_id = self.ws().focused_pane_id;
                self.ws().panes.get(&pane_id)
                    .map(|p| p.parser.lock().unwrap_or_else(|e| e.into_inner()).screen().contents())
                    .unwrap_or_default()
            };
            if content.trim().is_empty() {
                self.status_flash = Some(("No content to copy".to_string(), std::time::Instant::now()));
            } else {
                self.copy_to_clipboard(&content);
                let lines = content.lines().count();
                self.status_flash = Some((format!("Copied ({} lines)", lines), std::time::Instant::now()));
            }
            self.dirty = true;
            return Ok(true);
        }

        // New tab (configurable, default Ctrl+T)
        if self.key_matches(key, &self.config.keybindings.tab_new) {
            self.new_tab()?;
            return Ok(true);
        }

        // Next tab (configurable, default Alt+Right)
        if self.key_matches(key, &self.config.keybindings.tab_next) {
            if !self.workspaces.is_empty() {
                self.active_tab = (self.active_tab + 1) % self.workspaces.len();
            }
            return Ok(true);
        }

        // Previous tab (configurable, default Alt+Left)
        if self.key_matches(key, &self.config.keybindings.tab_prev) {
            if !self.workspaces.is_empty() {
                self.active_tab = if self.active_tab == 0 {
                    self.workspaces.len() - 1
                } else {
                    self.active_tab - 1
                };
            }
            return Ok(true);
        }

        // Alt+S — toggle status bar
        if key.modifiers == KeyModifiers::ALT
            && matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S'))
        {
            self.status_bar_visible = !self.status_bar_visible;
            self.mark_layout_change();
            return Ok(true);
        }

        // Toggle pane zoom (configurable, default Alt+Z)
        // When the preview has focus, Alt+Z zooms the preview instead (handled in handle_preview_key).
        if self.key_matches(key, &self.config.keybindings.zoom)
            && self.ws().focus_target != FocusTarget::Preview
        {
            self.toggle_zoom();
            return Ok(true);
        }

        // Toggle AI title generation (configurable, default Alt+A)
        if self.key_matches(key, &self.config.keybindings.ai_title_toggle) {
            self.ai_title_enabled = !self.ai_title_enabled;
            self.config.features.ai_title = self.ai_title_enabled;
            self.dirty = true;
            return Ok(true);
        }

        // Feature toggle dialog (configurable, default '?', pane focus only)
        if self.key_matches(key, &self.config.keybindings.feature_toggle)
            && self.ws().focus_target == FocusTarget::Pane
        {
            self.feature_toggle.visible = true;
            self.feature_toggle.selected = 0;
            self.feature_toggle.pending = self.config.features.clone();
            self.dirty = true;
            return Ok(true);
        }

        // Settings panel (configurable, default Ctrl+,)
        if self.key_matches(key, &self.config.keybindings.settings) {
            self.settings_panel.visible = true;
            self.settings_panel.selected = 0;
            self.settings_panel.editing = false;
            self.settings_panel.edit_buffer.clear();
            self.dirty = true;
            return Ok(true);
        }

        // Layout picker (configurable, default Ctrl+L)
        if self.key_matches(key, &self.config.keybindings.layout_picker) {
            if self.ws().layout.pane_count() > 1 {
                self.layout_picker.visible = true;
                self.layout_picker.selected = 0;
                return Ok(true);
            }
            return Ok(false);
        }

        // Alt+1 .. Alt+9 — jump to tab N
        if key.modifiers == KeyModifiers::ALT {
            if let KeyCode::Char(c) = key.code {
                if let Some(digit) = c.to_digit(10) {
                    if digit >= 1 && (digit as usize) <= self.workspaces.len() {
                        self.active_tab = (digit as usize) - 1;
                        return Ok(true);
                    }
                }
            }
        }

        // Directional pane focus (configurable, defaults Alt+h/j/k/l)
        if self.ws().focus_target == FocusTarget::Pane {
            if self.key_matches(key, &self.config.keybindings.pane_left) {
                self.focus_pane_in_direction(Direction::Left);
                return Ok(true);
            }
            if self.key_matches(key, &self.config.keybindings.pane_down) {
                self.focus_pane_in_direction(Direction::Down);
                return Ok(true);
            }
            if self.key_matches(key, &self.config.keybindings.pane_up) {
                self.focus_pane_in_direction(Direction::Up);
                return Ok(true);
            }
            if self.key_matches(key, &self.config.keybindings.pane_right) {
                self.focus_pane_in_direction(Direction::Right);
                return Ok(true);
            }
        }

        // Ctrl+Right / pane_next — next pane (cycle)
        if (key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Right)
            || self.key_matches(key, &self.config.keybindings.pane_next)
        {
            self.focus_next_pane();
            return Ok(true);
        }

        // Ctrl+Left / pane_prev — previous pane (cycle)
        if (key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Left)
            || self.key_matches(key, &self.config.keybindings.pane_prev)
        {
            self.focus_prev_pane();
            return Ok(true);
        }

        // Preview mode
        if self.ws().focus_target == FocusTarget::Preview {
            return self.handle_preview_key(key);
        }

        // File tree mode
        if self.ws().focus_target == FocusTarget::FileTree {
            if self.key_matches(key, &self.config.keybindings.file_tree) {
                self.toggle_file_tree();
                return Ok(true);
            }
            return self.handle_file_tree_key(key);
        }

        // Toggle file tree (configurable, default Ctrl+F)
        if self.key_matches(key, &self.config.keybindings.file_tree) {
            self.toggle_file_tree();
            return Ok(true);
        }

        // Swap preview and terminal positions (configurable, default Ctrl+P)
        if self.key_matches(key, &self.config.keybindings.preview_swap) {
            self.layout_swapped = !self.layout_swapped;
            return Ok(true);
        }

        let multi_pane = self.ws().layout.pane_count() > 1;
        let multi_tab = self.workspaces.len() > 1;

        // Vertical split (configurable, default Ctrl+D)
        if self.key_matches(key, &self.config.keybindings.split_vertical) {
            self.split_focused_pane(SplitDirection::Vertical)?;
            return Ok(true);
        }

        // Horizontal split (configurable, default Ctrl+E)
        if self.key_matches(key, &self.config.keybindings.split_horizontal) {
            self.split_focused_pane(SplitDirection::Horizontal)?;
            return Ok(true);
        }

        // Close pane / preview / tab (configurable, default Ctrl+W)
        if self.key_matches(key, &self.config.keybindings.pane_close) {
            if self.ws().focus_target == FocusTarget::Preview {
                self.preview_zoomed = false;
                self.ws_mut().preview.close();
                self.ws_mut().focus_target = FocusTarget::Pane;
                return Ok(true);
            }
            if multi_pane || multi_tab {
                let pane_id = self.ws().focused_pane_id;
                if self.config.worktree.close_confirm {
                    let worktree_path = self.ws().panes.get(&pane_id)
                        .and_then(|p| p.worktree_path.clone());
                    self.close_confirm_dialog = CloseConfirmDialog {
                        visible: true,
                        pane_id,
                        worktree_path,
                        focused: CloseConfirmFocus::No,
                    };
                    self.dirty = true;
                } else if multi_pane {
                    self.close_focused_pane();
                } else {
                    self.close_tab(self.active_tab);
                }
                return Ok(true);
            } else {
                return Ok(false);
            }
        }

        // Open pane create dialog (configurable, default Ctrl+N)
        if self.key_matches(key, &self.config.keybindings.pane_create) {
            self.pane_create_dialog = PaneCreateDialog {
                visible: true,
                worktree_enabled: self.config.worktree.auto_create,
                agent: self.config.startup.default_agent.clone(),
                focused_field: PaneCreateField::BranchName,
                base_branch: self.config.worktree.base_branch.clone(),
                ..Default::default()
            };
            self.dirty = true;
            return Ok(true);
        }

        Ok(false)
    }

    fn handle_rename_key(&mut self, key: KeyEvent) -> bool {
        let Some(buf) = self.rename_input.as_mut() else {
            return false;
        };
        let needs_relayout = !self.status_bar_visible;
        match key.code {
            KeyCode::Esc => {
                self.rename_input = None;
                if needs_relayout { self.mark_layout_change(); }
            }
            KeyCode::Enter => {
                let trimmed = buf.trim().to_string();
                self.ws_mut().custom_name = if trimmed.is_empty() { None } else { Some(trimmed) };
                self.rename_input = None;
                if needs_relayout { self.mark_layout_change(); }
            }
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c) => {
                // Ignore chars combined with Ctrl/Alt so shortcuts like
                // Ctrl+C don't leak into the buffer as literal letters.
                // Shift is fine — that's just uppercase.
                if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
                    return true;
                }
                // Cap at something sane so a stuck key can't grow the tab bar forever.
                if buf.chars().count() < 32 {
                    buf.push(c);
                }
            }
            _ => return true,
        }
        self.dirty = true;
        true
    }

    fn handle_file_tree_key(&mut self, key: KeyEvent) -> Result<bool> {
        // Ctrl+D / Ctrl+U — half-page scroll (5 lines), take priority over global splits.
        if key.modifiers == KeyModifiers::CONTROL {
            match key.code {
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    for _ in 0..5 { self.ws_mut().file_tree.move_down(); }
                    return Ok(true);
                }
                KeyCode::Char('u') | KeyCode::Char('U') => {
                    for _ in 0..5 { self.ws_mut().file_tree.move_up(); }
                    return Ok(true);
                }
                _ => {}
            }
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.ws_mut().file_tree.move_down();
                Ok(true)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.ws_mut().file_tree.move_up();
                Ok(true)
            }
            KeyCode::Enter | KeyCode::Char('o') => {
                let path = self.ws_mut().file_tree.toggle_or_select();
                if let Some(path) = path {
                    match self.config.filetree.enter_action.as_str() {
                        "neovim" | "editor" => {
                            self.open_in_editor(&path);
                        }
                        "choose" => {
                            self.filetree_action_popup = FileTreeActionPopup {
                                visible: true,
                                file_path: path,
                                selected: 0,
                            };
                            self.dirty = true;
                        }
                        other => {
                            if other != "preview" {
                                self.status_flash = Some((
                                    format!("unknown enter_action '{}'; falling back to preview", other),
                                    std::time::Instant::now(),
                                ));
                            }
                            self.clear_selection_if_preview();
                            let mut picker = self.image_picker.take();
                            self.ws_mut().preview.load(&path, picker.as_mut());
                            self.image_picker = picker;
                            // Shift focus to the preview so j/k/y/Y work immediately
                            self.ws_mut().focus_target = FocusTarget::Preview;
                        }
                    }
                }
                Ok(true)
            }
            KeyCode::Char('.') => {
                self.ws_mut().file_tree.toggle_hidden();
                Ok(true)
            }
            KeyCode::Char('d') => {
                // Diff preview toggle. Only meaningful when both the feature
                // is enabled in config AND a file is currently loaded.
                if self.config.features.diff_preview {
                    let had_diff = self.ws_mut().preview.toggle_diff();
                    if !had_diff {
                        self.status_flash = Some((
                            "no diff for selected file".to_string(),
                            std::time::Instant::now(),
                        ));
                    }
                }
                Ok(true)
            }
            KeyCode::Esc => {
                // Return to pane, keep preview open
                self.ws_mut().focus_target = FocusTarget::Pane;
                Ok(true)
            }
            _ => Ok(true),
        }
    }

    fn handle_preview_key(&mut self, key: KeyEvent) -> Result<bool> {
        // Close preview (configurable, default Ctrl+W)
        if self.key_matches(key, &self.config.keybindings.pane_close) {
            self.clear_selection_if_preview();
            self.preview_zoomed = false;
            self.ws_mut().preview.close();
            self.ws_mut().focus_target = FocusTarget::Pane;
            return Ok(true);
        }
        // Swap preview/terminal positions (configurable, default Ctrl+P)
        if self.key_matches(key, &self.config.keybindings.preview_swap) {
            self.layout_swapped = !self.layout_swapped;
            return Ok(true);
        }
        // Quit (configurable, default Ctrl+Q)
        if self.key_matches(key, &self.config.keybindings.quit) {
            self.should_quit = true;
            return Ok(true);
        }
        match (key.modifiers, key.code) {
            (_, KeyCode::Char('j')) | (_, KeyCode::Down) => {
                self.ws_mut().preview.scroll_down(1);
                Ok(true)
            }
            (_, KeyCode::Char('k')) | (_, KeyCode::Up) => {
                self.ws_mut().preview.scroll_up(1);
                Ok(true)
            }
            // Ctrl+D / Ctrl+U — half-page scroll (5 lines), overrides global split bindings.
            (KeyModifiers::CONTROL, KeyCode::Char('d')) | (KeyModifiers::CONTROL, KeyCode::Char('D')) => {
                self.ws_mut().preview.scroll_down(5);
                Ok(true)
            }
            (KeyModifiers::CONTROL, KeyCode::Char('u')) | (KeyModifiers::CONTROL, KeyCode::Char('U')) => {
                self.ws_mut().preview.scroll_up(5);
                Ok(true)
            }
            (_, KeyCode::PageDown) => {
                self.ws_mut().preview.scroll_down(20);
                Ok(true)
            }
            (_, KeyCode::PageUp) => {
                self.ws_mut().preview.scroll_up(20);
                Ok(true)
            }
            // Horizontal scroll — unmodified arrow keys and vim-style h/l.
            // Ctrl+Left/Right remain focus navigation (matched below).
            (KeyModifiers::NONE, KeyCode::Right)
            | (KeyModifiers::NONE, KeyCode::Char('l'))
            | (KeyModifiers::SHIFT, KeyCode::Right) => {
                self.ws_mut().preview.scroll_right(4);
                Ok(true)
            }
            (KeyModifiers::NONE, KeyCode::Left)
            | (KeyModifiers::NONE, KeyCode::Char('h'))
            | (KeyModifiers::SHIFT, KeyCode::Left) => {
                self.ws_mut().preview.scroll_left(4);
                Ok(true)
            }
            (KeyModifiers::NONE, KeyCode::Home) => {
                self.ws_mut().preview.h_scroll_offset = 0;
                Ok(true)
            }
            (_, KeyCode::Esc) => {
                self.preview_zoomed = false;
                // Return focus to the file tree if it's visible, otherwise to the pane.
                self.ws_mut().focus_target = if self.ws().file_tree_visible {
                    FocusTarget::FileTree
                } else {
                    FocusTarget::Pane
                };
                Ok(true)
            }
            (KeyModifiers::CONTROL, KeyCode::Right) => {
                self.focus_next_pane();
                Ok(true)
            }
            (KeyModifiers::CONTROL, KeyCode::Left) => {
                self.focus_prev_pane();
                Ok(true)
            }
            // y — copy filename to clipboard
            (KeyModifiers::NONE, KeyCode::Char('y')) => {
                if let Some(path) = self.ws().preview.file_path.clone() {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if !name.is_empty() {
                        self.copy_to_clipboard(&name);
                        self.status_flash = Some((
                            format!("Copied filename: {}", name),
                            std::time::Instant::now(),
                        ));
                    }
                }
                Ok(true)
            }
            // Y — copy full file path to clipboard (SHIFT+y or plain uppercase Y)
            (KeyModifiers::SHIFT, KeyCode::Char('Y'))
            | (KeyModifiers::NONE, KeyCode::Char('Y')) => {
                if let Some(path) = self.ws().preview.file_path.clone() {
                    let full = path.to_string_lossy().to_string();
                    self.copy_to_clipboard(&full);
                    self.status_flash = Some((
                        format!("Copied path: {}", full),
                        std::time::Instant::now(),
                    ));
                }
                Ok(true)
            }
            // Alt+Z — toggle preview zoom (full-screen preview)
            (KeyModifiers::ALT, KeyCode::Char('z')) | (KeyModifiers::ALT, KeyCode::Char('Z')) => {
                self.preview_zoomed = !self.preview_zoomed;
                self.mark_layout_change();
                Ok(true)
            }
            _ => Ok(true),
        }
    }


    fn new_tab(&mut self) -> Result<()> {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let name = dir_name(&cwd);
        let pane_id = self.next_pane_id;
        self.next_pane_id = self.next_pane_id.wrapping_add(1);

        let ws = Workspace::new(name, cwd, pane_id, 10, 40, self.event_tx.clone())?;
        self.workspaces.push(ws);
        self.active_tab = self.workspaces.len() - 1;
        Ok(())
    }

    fn close_tab(&mut self, index: usize) {
        if self.workspaces.len() <= 1 {
            return;
        }
        // Clean up claude monitor state for all panes in this tab
        let pane_ids: Vec<usize> = self.workspaces[index].panes.keys().copied().collect();
        for pane_id in pane_ids {
            self.claude_monitor.remove(pane_id);
        }
        self.workspaces[index].shutdown();
        self.workspaces.remove(index);
        if self.active_tab >= self.workspaces.len() {
            self.active_tab = self.workspaces.len() - 1;
        }
    }


    fn toggle_file_tree(&mut self) {
        let ws = self.ws_mut();
        let was_visible = ws.file_tree_visible;
        let will_be_visible;
        if ws.file_tree_visible && ws.focus_target == FocusTarget::FileTree {
            // Closing the tree — keep the preview open so the user can
            // continue reading the file they just opened. Focus moves
            // to the preview if it's active, otherwise back to the pane.
            ws.file_tree_visible = false;
            ws.focus_target = if ws.preview.is_active() {
                FocusTarget::Preview
            } else {
                FocusTarget::Pane
            };
            will_be_visible = false;
        } else if ws.file_tree_visible {
            ws.focus_target = FocusTarget::FileTree;
            will_be_visible = true;
        } else {
            ws.file_tree_visible = true;
            ws.focus_target = FocusTarget::FileTree;
            will_be_visible = true;
        }

        // Only relayout if the pane area actually changes (visibility flipped).
        if was_visible != will_be_visible {
            self.mark_layout_change();
        }
    }

    const MAX_PANES: usize = 16;
    const MIN_PANE_WIDTH: u16 = 20;
    const MIN_PANE_HEIGHT: u16 = 5;

    fn split_focused_pane(&mut self, direction: SplitDirection) -> Result<()> {
        if self.zoomed_pane_id.is_some() {
            return Ok(());
        }
        if self.ws().layout.pane_count() >= Self::MAX_PANES {
            return Ok(());
        }

        if let Some(&(_, rect)) = self
            .ws()
            .last_pane_rects
            .iter()
            .find(|(id, _)| *id == self.ws().focused_pane_id)
        {
            match direction {
                SplitDirection::Vertical => {
                    if rect.width / 2 < Self::MIN_PANE_WIDTH {
                        return Ok(());
                    }
                }
                SplitDirection::Horizontal => {
                    if rect.height / 2 < Self::MIN_PANE_HEIGHT {
                        return Ok(());
                    }
                }
            }
        }

        let new_id = self.next_pane_id;
        self.next_pane_id = self.next_pane_id.wrapping_add(1);

        // Inherit CWD from the focused pane
        let parent_cwd = self.ws().panes.get(&self.ws().focused_pane_id)
            .map(|p| p.cwd.clone());

        let pane = Pane::new_with_cwd(new_id, 10, 40, self.event_tx.clone(), parent_cwd)?;
        let ws = self.ws_mut();
        ws.panes.insert(new_id, pane);
        ws.layout.split_pane(ws.focused_pane_id, new_id, direction);
        // Focus moves to the freshly-created pane so the user can type
        // in it immediately after splitting.
        ws.focused_pane_id = new_id;

        self.mark_layout_change();
        Ok(())
    }

    fn close_focused_pane(&mut self) {
        // If zoomed, restore the saved layout first so remove_pane operates on the
        // real multi-pane tree, not the single-leaf zoom overlay.
        if self.zoomed_pane_id.is_some() {
            if let Some(saved_layout) = self.pre_zoom_layout.take() {
                self.ws_mut().layout = saved_layout;
            }
            self.zoomed_pane_id = None;
        }
        let focused = self.ws().focused_pane_id;
        let ws = self.ws_mut();
        if ws.layout.pane_count() <= 1 {
            return;
        }

        let pane_ids = ws.layout.collect_pane_ids();
        let current_idx = pane_ids.iter().position(|&id| id == focused);

        ws.layout.remove_pane(focused);

        if let Some(mut pane) = ws.panes.remove(&focused) {
            pane.kill();
        }

        // Clean up state maps for this pane
        self.claude_monitor.remove(focused);
        self.pane_states.remove(&focused);
        self.pane_output_rings.remove(&focused);
        self.ai_titles.remove(&focused);
        self.last_ai_title_request.remove(&focused);
        self.ai_title_in_flight.remove(&focused);
        let ws = self.ws_mut();

        let remaining_ids = ws.layout.collect_pane_ids();
        if let Some(idx) = current_idx {
            let new_idx = if idx >= remaining_ids.len() {
                remaining_ids.len().saturating_sub(1)
            } else {
                idx
            };
            ws.focused_pane_id = remaining_ids[new_idx];
        } else if let Some(&first) = remaining_ids.first() {
            ws.focused_pane_id = first;
        }

        self.mark_layout_change();
    }

    fn toggle_zoom(&mut self) {
        if self.zoomed_pane_id.is_some() {
            if let Some(saved_layout) = self.pre_zoom_layout.take() {
                self.ws_mut().layout = saved_layout;
            }
            self.zoomed_pane_id = None;
            self.pre_zoom_layout = None;
        } else {
            let focused = self.ws().focused_pane_id;
            if self.ws().layout.pane_count() > 1 {
                self.pre_zoom_layout = Some(self.ws().layout.clone_layout());
                self.ws_mut().layout = LayoutNode::Leaf { pane_id: focused };
                self.zoomed_pane_id = Some(focused);
            }
        }
        self.mark_layout_change();
    }

    pub fn apply_layout_mode(&mut self, mode: LayoutMode) {
        let pane_ids = self.ws().layout.collect_pane_ids();
        if pane_ids.is_empty() {
            return;
        }

        let new_layout = Self::build_layout_node(mode, &pane_ids);
        if let Some(layout) = new_layout {
            self.ws_mut().layout = layout;
            self.ws_mut().layout_mode = mode;
            self.mark_layout_change();
        }
    }

    fn build_layout_node(mode: LayoutMode, pane_ids: &[usize]) -> Option<LayoutNode> {
        let count = pane_ids.len();
        if count == 0 {
            return None;
        }
        if count == 1 {
            return Some(LayoutNode::Leaf { pane_id: pane_ids[0] });
        }

        match mode {
            LayoutMode::Stack | LayoutMode::Auto => {
                Self::build_stack(pane_ids, SplitDirection::Horizontal)
            }
            LayoutMode::TwoSplit => {
                let left = LayoutNode::Leaf { pane_id: pane_ids[0] };
                let right = if count == 2 {
                    LayoutNode::Leaf { pane_id: pane_ids[1] }
                } else {
                    Self::build_stack(&pane_ids[1..], SplitDirection::Horizontal)
                        .unwrap_or(LayoutNode::Leaf { pane_id: pane_ids[1] })
                };
                Some(LayoutNode::Split {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                    first: Box::new(left),
                    second: Box::new(right),
                })
            }
            LayoutMode::Grid => {
                if count >= 4 {
                    let top = LayoutNode::Split {
                        direction: SplitDirection::Vertical,
                        ratio: 0.5,
                        first: Box::new(LayoutNode::Leaf { pane_id: pane_ids[0] }),
                        second: Box::new(LayoutNode::Leaf { pane_id: pane_ids[1] }),
                    };
                    let bottom = LayoutNode::Split {
                        direction: SplitDirection::Vertical,
                        ratio: 0.5,
                        first: Box::new(LayoutNode::Leaf { pane_id: pane_ids[2] }),
                        second: Box::new(LayoutNode::Leaf { pane_id: pane_ids[3] }),
                    };
                    Some(LayoutNode::Split {
                        direction: SplitDirection::Horizontal,
                        ratio: 0.5,
                        first: Box::new(top),
                        second: Box::new(bottom),
                    })
                } else if count == 3 {
                    let top = LayoutNode::Split {
                        direction: SplitDirection::Vertical,
                        ratio: 0.5,
                        first: Box::new(LayoutNode::Leaf { pane_id: pane_ids[0] }),
                        second: Box::new(LayoutNode::Leaf { pane_id: pane_ids[1] }),
                    };
                    Some(LayoutNode::Split {
                        direction: SplitDirection::Horizontal,
                        ratio: 0.5,
                        first: Box::new(top),
                        second: Box::new(LayoutNode::Leaf { pane_id: pane_ids[2] }),
                    })
                } else {
                    Self::build_layout_node(LayoutMode::TwoSplit, pane_ids)
                }
            }
            LayoutMode::MainSub => {
                if count >= 3 {
                    let main = LayoutNode::Leaf { pane_id: pane_ids[0] };
                    let sub = Self::build_stack(&pane_ids[1..], SplitDirection::Horizontal)
                        .unwrap_or(LayoutNode::Leaf { pane_id: pane_ids[1] });
                    Some(LayoutNode::Split {
                        direction: SplitDirection::Vertical,
                        ratio: 0.6,
                        first: Box::new(main),
                        second: Box::new(sub),
                    })
                } else {
                    Self::build_layout_node(LayoutMode::TwoSplit, pane_ids)
                }
            }
            LayoutMode::BigOnePlusThree => {
                if count >= 4 {
                    let big = LayoutNode::Leaf { pane_id: pane_ids[0] };
                    let small = Self::build_stack(&pane_ids[1..4], SplitDirection::Horizontal)
                        .unwrap_or(LayoutNode::Leaf { pane_id: pane_ids[1] });
                    Some(LayoutNode::Split {
                        direction: SplitDirection::Vertical,
                        ratio: 0.65,
                        first: Box::new(big),
                        second: Box::new(small),
                    })
                } else if count == 3 {
                    Self::build_layout_node(LayoutMode::MainSub, pane_ids)
                } else {
                    Self::build_layout_node(LayoutMode::TwoSplit, pane_ids)
                }
            }
        }
    }

    fn build_stack(pane_ids: &[usize], direction: SplitDirection) -> Option<LayoutNode> {
        match pane_ids.len() {
            0 => None,
            1 => Some(LayoutNode::Leaf { pane_id: pane_ids[0] }),
            2 => Some(LayoutNode::Split {
                direction,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf { pane_id: pane_ids[0] }),
                second: Box::new(LayoutNode::Leaf { pane_id: pane_ids[1] }),
            }),
            _ => {
                let mid = pane_ids.len() / 2;
                let first = Self::build_stack(&pane_ids[..mid], direction)?;
                let second = Self::build_stack(&pane_ids[mid..], direction)?;
                Some(LayoutNode::Split {
                    direction,
                    ratio: mid as f32 / pane_ids.len() as f32,
                    first: Box::new(first),
                    second: Box::new(second),
                })
            }
        }
    }

    fn cycle_layout_mode(&mut self) {
        let count = self.ws().layout.pane_count();
        if count <= 1 {
            return;
        }

        let current = self.ws().layout_mode;
        let modes: &[LayoutMode] = if count == 2 {
            &[LayoutMode::Stack, LayoutMode::TwoSplit]
        } else if count == 3 {
            &[LayoutMode::Stack, LayoutMode::TwoSplit, LayoutMode::MainSub]
        } else {
            &[
                LayoutMode::Stack,
                LayoutMode::TwoSplit,
                LayoutMode::Grid,
                LayoutMode::MainSub,
                LayoutMode::BigOnePlusThree,
            ]
        };

        let next = if current == LayoutMode::Auto {
            modes[0]
        } else {
            modes
                .iter()
                .position(|&m| m == current)
                .map(|idx| modes[(idx + 1) % modes.len()])
                .unwrap_or(modes[0])
        };

        self.apply_layout_mode(next);
    }

    fn handle_layout_picker_key(&mut self, key: KeyEvent) -> Result<bool> {
        if self.key_matches(key, &self.config.keybindings.quit) {
            self.should_quit = true;
            return Ok(true);
        }

        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Char('1')) => {
                self.apply_layout_mode(LayoutMode::Stack);
                self.layout_picker.visible = false;
            }
            (KeyModifiers::NONE, KeyCode::Char('2')) => {
                self.apply_layout_mode(LayoutMode::TwoSplit);
                self.layout_picker.visible = false;
            }
            (KeyModifiers::NONE, KeyCode::Char('3')) => {
                self.apply_layout_mode(LayoutMode::Grid);
                self.layout_picker.visible = false;
            }
            (KeyModifiers::NONE, KeyCode::Char('4')) => {
                self.apply_layout_mode(LayoutMode::MainSub);
                self.layout_picker.visible = false;
            }
            (KeyModifiers::NONE, KeyCode::Char('5')) => {
                self.apply_layout_mode(LayoutMode::BigOnePlusThree);
                self.layout_picker.visible = false;
            }
            (KeyModifiers::NONE, KeyCode::Char('6')) => {
                self.apply_layout_mode(LayoutMode::Auto);
                self.layout_picker.visible = false;
            }
            (KeyModifiers::NONE, KeyCode::Esc)
            | (KeyModifiers::NONE, KeyCode::Char('q')) => {
                self.layout_picker.visible = false;
            }
            (KeyModifiers::NONE, KeyCode::Char('j')) | (KeyModifiers::NONE, KeyCode::Down) => {
                self.layout_picker.selected = (self.layout_picker.selected + 1) % 6;
            }
            (KeyModifiers::NONE, KeyCode::Char('k')) | (KeyModifiers::NONE, KeyCode::Up) => {
                self.layout_picker.selected = self.layout_picker.selected.saturating_sub(1);
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                let mode = match self.layout_picker.selected {
                    0 => LayoutMode::Stack,
                    1 => LayoutMode::TwoSplit,
                    2 => LayoutMode::Grid,
                    3 => LayoutMode::MainSub,
                    4 => LayoutMode::BigOnePlusThree,
                    _ => LayoutMode::Auto,
                };
                self.apply_layout_mode(mode);
                self.layout_picker.visible = false;
            }
            (KeyModifiers::NONE, KeyCode::Char(' ')) => {
                self.layout_picker.selected = (self.layout_picker.selected + 1) % 6;
            }
            _ => {}
        }
        self.dirty = true;
        Ok(true)
    }

    fn handle_feature_toggle_key(&mut self, key: KeyEvent) -> Result<bool> {
        if self.key_matches(key, &self.config.keybindings.quit) {
            self.should_quit = true;
            return Ok(true);
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.feature_toggle.selected =
                    (self.feature_toggle.selected + 1) % FEATURES.len();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.feature_toggle.selected = if self.feature_toggle.selected == 0 {
                    FEATURES.len() - 1
                } else {
                    self.feature_toggle.selected - 1
                };
            }
            KeyCode::Char(' ') => {
                let (key_name, _) = FEATURES[self.feature_toggle.selected];
                let current = self.feature_toggle.pending.get_by_key(key_name);
                self.feature_toggle.pending.set_by_key(key_name, !current);
            }
            // Only close dialog on plain 'q' (no modifiers) or Enter
            KeyCode::Char('q') if key.modifiers == KeyModifiers::NONE => {
                self.config.features = self.feature_toggle.pending.clone();
                self.ai_title_enabled = self.config.features.ai_title;
                self.feature_toggle.visible = false;
                if let Err(e) = self.config.save() {
                    eprintln!("glowmux: config save error: {}", e);
                }
            }
            KeyCode::Enter => {
                self.config.features = self.feature_toggle.pending.clone();
                self.ai_title_enabled = self.config.features.ai_title;
                self.feature_toggle.visible = false;
                if let Err(e) = self.config.save() {
                    eprintln!("glowmux: config save error: {}", e);
                }
            }
            KeyCode::Esc => {
                self.feature_toggle.visible = false;
            }
            _ => {}
        }
        self.dirty = true;
        Ok(true)
    }

    fn handle_settings_panel_key(&mut self, key: KeyEvent) -> Result<bool> {
        if self.settings_panel.editing {
            match key.code {
                KeyCode::Enter => {
                    let Some(&(key_name, _)) = SETTINGS_ITEMS.get(self.settings_panel.selected) else {
                        self.settings_panel.editing = false;
                        self.settings_panel.edit_buffer.clear();
                        self.dirty = true;
                        return Ok(true);
                    };
                    let buf = self.settings_panel.edit_buffer.clone();
                    self.set_setting_value(key_name, &buf);
                    if let Err(e) = self.config.save() {
                        eprintln!("glowmux: config save error: {}", e);
                    }
                    self.settings_panel.editing = false;
                    self.settings_panel.edit_buffer.clear();
                }
                KeyCode::Esc => {
                    self.settings_panel.editing = false;
                    self.settings_panel.edit_buffer.clear();
                }
                KeyCode::Backspace => {
                    self.settings_panel.edit_buffer.pop();
                }
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    if self.settings_panel.edit_buffer.len() < 10 {
                        self.settings_panel.edit_buffer.push(c);
                    }
                }
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.settings_panel.selected =
                        (self.settings_panel.selected + 1) % SETTINGS_ITEMS.len();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.settings_panel.selected = if self.settings_panel.selected == 0 {
                        SETTINGS_ITEMS.len() - 1
                    } else {
                        self.settings_panel.selected - 1
                    };
                }
                KeyCode::Enter => {
                    if let Some(&(key_name, _)) = SETTINGS_ITEMS.get(self.settings_panel.selected) {
                        self.settings_panel.edit_buffer = self.get_setting_value(key_name);
                        self.settings_panel.editing = true;
                    }
                }
                KeyCode::Char('q') | KeyCode::Esc => {
                    self.settings_panel.visible = false;
                }
                _ => {}
            }
        }
        self.dirty = true;
        Ok(true)
    }

    pub fn get_setting_value(&self, key: &str) -> String {
        match key {
            "terminal.scrollback" => self.config.terminal.scrollback.to_string(),
            "layout.breakpoint_stack" => self.config.layout.breakpoint_stack.to_string(),
            "layout.breakpoint_split2" => self.config.layout.breakpoint_split2.to_string(),
            "ai_title_engine.update_interval_sec" => self.config.ai_title_engine.update_interval_sec.to_string(),
            "ai_title_engine.max_chars" => self.config.ai_title_engine.max_chars.to_string(),
            _ => String::new(),
        }
    }

    fn set_setting_value(&mut self, key: &str, value: &str) {
        match key {
            "terminal.scrollback" => {
                if let Ok(v) = value.parse::<usize>() {
                    self.config.terminal.scrollback = v.clamp(100, 1_000_000);
                }
            }
            "layout.breakpoint_stack" => {
                if let Ok(v) = value.parse::<u16>() {
                    self.config.layout.breakpoint_stack = v.max(40);
                }
            }
            "layout.breakpoint_split2" => {
                if let Ok(v) = value.parse::<u16>() {
                    self.config.layout.breakpoint_split2 = v.max(40);
                }
            }
            "ai_title_engine.update_interval_sec" => {
                if let Ok(v) = value.parse::<u64>() {
                    self.config.ai_title_engine.update_interval_sec = v.max(5);
                }
            }
            "ai_title_engine.max_chars" => {
                if let Ok(v) = value.parse::<usize>() {
                    self.config.ai_title_engine.max_chars = v.clamp(5, 200);
                }
            }
            _ => {}
        }
    }

    fn handle_pane_create_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc => {
                self.pane_create_dialog.visible = false;
                self.dirty = true;
            }
            KeyCode::Tab => {
                self.pane_create_dialog.focused_field = match self.pane_create_dialog.focused_field {
                    PaneCreateField::BranchName => PaneCreateField::BaseBranch,
                    PaneCreateField::BaseBranch => PaneCreateField::WorktreeToggle,
                    PaneCreateField::WorktreeToggle => PaneCreateField::AgentField,
                    PaneCreateField::AgentField => PaneCreateField::AiGenerate,
                    PaneCreateField::AiGenerate => PaneCreateField::OkButton,
                    PaneCreateField::OkButton => PaneCreateField::CancelButton,
                    PaneCreateField::CancelButton => PaneCreateField::BranchName,
                };
                self.dirty = true;
            }
            KeyCode::BackTab => {
                self.pane_create_dialog.focused_field = match self.pane_create_dialog.focused_field {
                    PaneCreateField::BranchName => PaneCreateField::CancelButton,
                    PaneCreateField::BaseBranch => PaneCreateField::BranchName,
                    PaneCreateField::WorktreeToggle => PaneCreateField::BaseBranch,
                    PaneCreateField::AgentField => PaneCreateField::WorktreeToggle,
                    PaneCreateField::AiGenerate => PaneCreateField::AgentField,
                    PaneCreateField::OkButton => PaneCreateField::AiGenerate,
                    PaneCreateField::CancelButton => PaneCreateField::OkButton,
                };
                self.dirty = true;
            }
            KeyCode::Enter => {
                let field = self.pane_create_dialog.focused_field.clone();
                match field {
                    PaneCreateField::CancelButton => {
                        self.pane_create_dialog.visible = false;
                        self.dirty = true;
                    }
                    PaneCreateField::WorktreeToggle => {
                        self.pane_create_dialog.worktree_enabled =
                            !self.pane_create_dialog.worktree_enabled;
                        self.dirty = true;
                    }
                    PaneCreateField::AiGenerate => {
                        if !self.pane_create_dialog.generating_name {
                            self.start_branch_name_generation();
                        }
                    }
                    PaneCreateField::OkButton | PaneCreateField::BranchName | PaneCreateField::BaseBranch => {
                        if !self.pane_create_dialog.generating_name {
                            let branch = self.pane_create_dialog.branch_name.clone();
                            let worktree = self.pane_create_dialog.worktree_enabled;
                            let agent = self.pane_create_dialog.agent.clone();
                            let base_branch = self.pane_create_dialog.base_branch.clone();
                            self.pane_create_dialog.visible = false;
                            self.create_pane_from_dialog(branch, worktree, agent, base_branch)?;
                            self.dirty = true;
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Backspace => {
                match self.pane_create_dialog.focused_field {
                    PaneCreateField::BranchName => {
                        self.pane_create_dialog.branch_name.pop();
                        self.dirty = true;
                    }
                    PaneCreateField::BaseBranch => {
                        self.pane_create_dialog.base_branch.pop();
                        self.dirty = true;
                    }
                    PaneCreateField::AgentField => {
                        self.pane_create_dialog.agent.pop();
                        self.dirty = true;
                    }
                    _ => {}
                }
            }
            KeyCode::Char(c) => {
                match self.pane_create_dialog.focused_field {
                    PaneCreateField::BranchName => {
                        if c.is_ascii_alphanumeric() || c == '-' || c == '/' || c == '_' {
                            self.pane_create_dialog.branch_name.push(c);
                            self.dirty = true;
                        }
                    }
                    PaneCreateField::BaseBranch => {
                        if c.is_ascii_alphanumeric() || c == '-' || c == '/' || c == '_' || c == '.' {
                            self.pane_create_dialog.base_branch.push(c);
                            self.dirty = true;
                        }
                    }
                    PaneCreateField::AgentField => {
                        // Allow printable ASCII for agent command (except newline/null)
                        if c.is_ascii_graphic() || c == ' ' {
                            self.pane_create_dialog.agent.push(c);
                            self.dirty = true;
                        }
                    }
                    PaneCreateField::WorktreeToggle if c == ' ' => {
                        self.pane_create_dialog.worktree_enabled =
                            !self.pane_create_dialog.worktree_enabled;
                        self.dirty = true;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(true)
    }

    fn handle_close_confirm_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                self.close_confirm_dialog.visible = false;
                self.dirty = true;
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.close_confirm_dialog.focused = CloseConfirmFocus::Yes;
                self.dirty = true;
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.close_confirm_dialog.focused = CloseConfirmFocus::No;
                self.dirty = true;
            }
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                let should_close = self.close_confirm_dialog.focused == CloseConfirmFocus::Yes
                    || matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y'));
                if should_close {
                    let pane_id = self.close_confirm_dialog.pane_id;
                    let worktree_path = self.close_confirm_dialog.worktree_path.clone();
                    self.close_confirm_dialog.visible = false;
                    if let Some(wt_path) = worktree_path {
                        match self.config.worktree.close_worktree.as_str() {
                            "auto" => {
                                let repo_root = self.ws().cwd.clone();
                                if let Some(handle) = &self.tokio_handle {
                                    let path = wt_path.clone();
                                    let root = repo_root.clone();
                                    handle.spawn(async move {
                                        let result = tokio::task::spawn_blocking(move || {
                                            crate::worktree::WorktreeManager::new().remove(&path, &root)
                                        }).await;
                                        if let Ok(Err(e)) = result {
                                            eprintln!("glowmux: worktree remove failed: {}", e);
                                        }
                                    });
                                }
                            }
                            "ask" => {
                                self.worktree_cleanup_dialog = Some(WorktreeCleanupDialog {
                                    visible: true,
                                    worktree_path: wt_path.clone(),
                                    branch: self.ws().panes.values()
                                        .find(|p| p.worktree_path.as_ref() == Some(&wt_path))
                                        .and_then(|p| p.branch_name.clone())
                                        .unwrap_or_default(),
                                    focused: CloseConfirmFocus::No,
                                });
                            }
                            _ => {} // "never"
                        }
                    }
                    let multi_pane = self.ws().layout.pane_count() > 1;
                    let multi_tab = self.workspaces.len() > 1;
                    if multi_pane {
                        self.ws_mut().focused_pane_id = pane_id;
                        self.close_focused_pane();
                    } else if multi_tab {
                        self.close_tab(self.active_tab);
                    }
                    self.dirty = true;
                } else {
                    self.close_confirm_dialog.visible = false;
                    self.dirty = true;
                }
            }
            _ => {}
        }
        Ok(true)
    }

    fn handle_worktree_cleanup_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                if let Some(ref mut d) = self.worktree_cleanup_dialog {
                    d.visible = false;
                }
                self.dirty = true;
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if let Some(ref mut d) = self.worktree_cleanup_dialog {
                    d.focused = CloseConfirmFocus::Yes;
                }
                self.dirty = true;
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if let Some(ref mut d) = self.worktree_cleanup_dialog {
                    d.focused = CloseConfirmFocus::No;
                }
                self.dirty = true;
            }
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                let should_delete = self
                    .worktree_cleanup_dialog
                    .as_ref()
                    .map(|d| {
                        d.focused == CloseConfirmFocus::Yes
                            || matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y'))
                    })
                    .unwrap_or(false);
                if should_delete {
                    let path = self
                        .worktree_cleanup_dialog
                        .as_ref()
                        .map(|d| d.worktree_path.clone());
                    if let Some(p) = path {
                        let repo_root = self.ws().cwd.clone();
                        if let Some(handle) = &self.tokio_handle {
                            let root = repo_root.clone();
                            handle.spawn(async move {
                                let result = tokio::task::spawn_blocking(move || {
                                    crate::worktree::WorktreeManager::new().remove(&p, &root)
                                }).await;
                                if let Ok(Err(e)) = result {
                                    eprintln!("glowmux: worktree remove failed: {}", e);
                                }
                            });
                        }
                    }
                }
                if let Some(ref mut d) = self.worktree_cleanup_dialog {
                    d.visible = false;
                }
                self.dirty = true;
            }
            _ => {}
        }
        Ok(true)
    }

    fn start_branch_name_generation(&mut self) {
        // Respect the feature gate: worktree_ai_name OR ai.worktree_name.enabled.
        // Either flag turns the AI Generate button into a real call.
        let feature_on = self.config.features.worktree_ai_name
            || self.config.ai.worktree_name.enabled;
        if !feature_on {
            self.pane_create_dialog.error_msg =
                Some("AI worktree name disabled (features.worktree_ai_name)".to_string());
            self.dirty = true;
            return;
        }
        if let Some(handle) = &self.tokio_handle {
            self.pane_create_dialog.generating_name = true;
            self.pane_create_dialog.error_msg = None;
            let tx = self.event_tx.clone();
            let config = self.config.ai.clone();
            let context = self.pane_create_dialog.branch_name.clone();
            handle.spawn(async move {
                let result = crate::worktree::generate_branch_name(&context, &config).await;
                let branch = result.unwrap_or_default();
                let _ = tx.send(AppEvent::BranchNameGenerated { branch });
            });
        }
    }

    fn create_pane_from_dialog(
        &mut self,
        branch_name: String,
        worktree_enabled: bool,
        agent: String,
        base_branch: String,
    ) -> Result<()> {
        let (cols, rows) = self.last_term_size;
        let pane_id = self.next_pane_id;
        self.next_pane_id = self.next_pane_id.wrapping_add(1);

        let cwd = self.ws().cwd.clone();
        let pane_rows = rows.saturating_sub(5);
        let pane_cols = cols.saturating_sub(2);
        let mut pane = crate::pane::Pane::new(
            pane_id, pane_rows, pane_cols, self.event_tx.clone(),
        )?;

        if !branch_name.is_empty() {
            pane.branch_name = Some(branch_name.clone());
        }

        self.ws_mut().panes.insert(pane_id, pane);

        let focused_id = self.ws().focused_pane_id;
        self.ws_mut()
            .layout
            .split_pane(focused_id, pane_id, SplitDirection::Vertical);
        self.ws_mut().focused_pane_id = pane_id;
        self.mark_layout_change();

        let has_branch = !branch_name.is_empty();
        if worktree_enabled && has_branch {
            if let Some(handle) = &self.tokio_handle {
                let tx = self.event_tx.clone();
                let repo_root = cwd;
                let branch = branch_name;
                let opts = crate::worktree::WorktreeCreateOptions {
                    prefer_gwq: self.config.worktree.prefer_gwq,
                    worktree_dir: self.config.worktree.worktree_dir.clone(),
                    base_branch,
                };
                handle.spawn(async move {
                    let branch_clone = branch.clone();
                    let opts_clone = opts.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        let mgr = crate::worktree::WorktreeManager::new();
                        mgr.create_with_options(&repo_root, &branch_clone, &opts_clone)
                    }).await;
                    match result {
                        Ok(Ok(path)) => {
                            let _ = tx.send(AppEvent::WorktreeCreated { pane_id, cwd: path, branch_name: branch });
                        }
                        Ok(Err(e)) => {
                            let _ = tx.send(AppEvent::WorktreeCreateFailed { pane_id, branch_name: branch, error: e.to_string() });
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::WorktreeCreateFailed { pane_id, branch_name: branch, error: e.to_string() });
                        }
                    }
                });
            }
        } else if !worktree_enabled && has_branch {
            // Non-worktree branch: run `git checkout -b <branch>` in the new pane's shell.
            // Shell-quote the branch name to handle special characters safely.
            let quoted = format!("'{}'", branch_name.replace('\'', "'\\''"));
            let cmd = format!("git checkout -b {}\n", quoted);
            if let Some(pane) = self.ws_mut().panes.get_mut(&pane_id) {
                let _ = pane.write_input(cmd.as_bytes());
            }
        }

        // If an agent command is set, launch it in the new pane after any branch/worktree setup.
        // For worktree panes the agent will be launched after the WorktreeCreated cd completes,
        // so we store it on the pane and send it in the WorktreeCreated handler.
        // For non-worktree panes we send it immediately (shell buffers the input).
        if !agent.is_empty() {
            if worktree_enabled && has_branch {
                // Store for deferred launch after WorktreeCreated cd
                if let Some(pane) = self.ws_mut().panes.get_mut(&pane_id) {
                    pane.pending_agent = Some(agent);
                }
            } else {
                let cmd = format!("{}\n", agent);
                if let Some(pane) = self.ws_mut().panes.get_mut(&pane_id) {
                    let _ = pane.write_input(cmd.as_bytes());
                }
            }
        }

        Ok(())
    }

    pub fn save_session(&self) {
        if !self.config.session.enabled {
            return;
        }
        let Some(path) = crate::session::SessionData::session_path() else {
            return;
        };

        let workspaces: Vec<_> = self
            .workspaces
            .iter()
            .map(|ws| {
                let panes: Vec<_> = ws
                    .panes
                    .values()
                    .map(|p| crate::session::PaneSnapshot {
                        id: p.id,
                        cwd: p.pane_cwd(),
                        title: self.ai_titles.get(&p.id).cloned().unwrap_or_default(),
                        worktree_path: p.worktree_path.clone(),
                        branch: p.branch_name.clone(),
                    })
                    .collect();
                crate::session::WorkspaceSnapshot {
                    name: ws.name.clone(),
                    cwd: ws.cwd.clone(),
                    panes,
                    layout_mode: format!("{:?}", ws.layout_mode),
                }
            })
            .collect();

        let data = crate::session::SessionData {
            version: 1,
            workspaces,
            active_tab: self.active_tab,
        };
        data.save(&path);
    }

    fn apply_startup_panes(&mut self, rows: u16, cols: u16) -> Result<()> {
        let pane_configs = self.config.startup.panes.clone();

        for (i, startup_pane) in pane_configs.iter().enumerate().skip(1) {
            let new_id = self.next_pane_id;
            self.next_pane_id = self.next_pane_id.wrapping_add(1);

            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let pane = Pane::new_with_cwd(new_id, rows, cols, self.event_tx.clone(), Some(cwd))?;

            let focused = self.ws().focused_pane_id;
            let ws = self.ws_mut();
            ws.panes.insert(new_id, pane);

            let direction = if i % 2 == 1 {
                SplitDirection::Vertical
            } else {
                SplitDirection::Horizontal
            };
            ws.layout.split_pane(focused, new_id, direction);
            ws.focused_pane_id = new_id;

            if !startup_pane.command.is_empty() {
                let cmd = format!("{}\r", startup_pane.command);
                if let Some(p) = ws.panes.get_mut(&new_id) {
                    let _ = p.write_input(cmd.as_bytes());
                }
            }
        }

        if let Some(first) = pane_configs.first() {
            if !first.command.is_empty() {
                let Some(&first_id) = self.workspaces[0].layout.collect_pane_ids().first() else {
                    return Ok(());
                };
                let cmd = format!("{}\r", first.command);
                if let Some(p) = self.ws_mut().panes.get_mut(&first_id) {
                    let _ = p.write_input(cmd.as_bytes());
                }
            }
        }

        if let Some(&first_id) = self.ws().layout.collect_pane_ids().first() {
            self.ws_mut().focused_pane_id = first_id;
        }

        Ok(())
    }

    fn focus_pane_in_direction(&mut self, dir: Direction) {
        let focused = self.ws().focused_pane_id;

        let Some(&(_, current_rect)) = self.ws().last_pane_rects.iter()
            .find(|(id, _)| *id == focused)
        else {
            return;
        };

        let cx = current_rect.x as i32 + current_rect.width as i32 / 2;
        let cy = current_rect.y as i32 + current_rect.height as i32 / 2;

        let mut best_id: Option<usize> = None;
        let mut best_dist = i32::MAX;

        for &(pane_id, rect) in &self.ws().last_pane_rects {
            if pane_id == focused {
                continue;
            }

            let px = rect.x as i32 + rect.width as i32 / 2;
            let py = rect.y as i32 + rect.height as i32 / 2;

            let is_candidate = match dir {
                Direction::Left  => px < cx && rect.x as i32 + rect.width as i32 <= current_rect.x as i32 + 2,
                Direction::Right => px > cx && rect.x as i32 >= current_rect.x as i32 + current_rect.width as i32 - 2,
                Direction::Up    => py < cy && rect.y as i32 + rect.height as i32 <= current_rect.y as i32 + 2,
                Direction::Down  => py > cy && rect.y as i32 >= current_rect.y as i32 + current_rect.height as i32 - 2,
            };

            if !is_candidate {
                continue;
            }

            let dist = (px - cx).abs() + (py - cy).abs();
            if dist < best_dist {
                best_dist = dist;
                best_id = Some(pane_id);
            }
        }

        if let Some(new_id) = best_id {
            self.dismiss_done_on_focus(new_id);
            self.ws_mut().focused_pane_id = new_id;
            self.dirty = true;
        }
    }

    /// Cycle focus forward: FileTree → Preview → Pane1 → Pane2 → ... → FileTree
    fn focus_next_pane(&mut self) {
        let ws = self.ws_mut();
        let ids = ws.layout.collect_pane_ids();
        let tree_visible = ws.file_tree_visible;
        let preview_active = ws.preview.is_active();
        let _swapped = false; // preview position doesn't affect focus order

        match ws.focus_target {
            FocusTarget::FileTree => {
                // File tree → preview (if active) or first pane
                if preview_active {
                    ws.focus_target = FocusTarget::Preview;
                } else {
                    ws.focus_target = FocusTarget::Pane;
                }
            }
            FocusTarget::Preview => {
                // Preview → first pane
                ws.focus_target = FocusTarget::Pane;
            }
            FocusTarget::Pane => {
                if let Some(idx) = ids.iter().position(|&id| id == ws.focused_pane_id) {
                    if idx + 1 < ids.len() {
                        ws.focused_pane_id = ids[idx + 1];
                    } else if tree_visible {
                        ws.focus_target = FocusTarget::FileTree;
                    } else if preview_active {
                        ws.focus_target = FocusTarget::Preview;
                    } else {
                        ws.focused_pane_id = ids[0];
                    }
                }
            }
        }
        let new_id = self.ws().focused_pane_id;
        self.dismiss_done_on_focus(new_id);
    }

    /// Cycle focus backward
    fn focus_prev_pane(&mut self) {
        let ws = self.ws_mut();
        let ids = ws.layout.collect_pane_ids();
        let tree_visible = ws.file_tree_visible;
        let preview_active = ws.preview.is_active();

        match ws.focus_target {
            FocusTarget::FileTree => {
                // File tree → last pane
                ws.focus_target = FocusTarget::Pane;
                if let Some(&last) = ids.last() {
                    ws.focused_pane_id = last;
                }
            }
            FocusTarget::Preview => {
                // Preview → file tree (if visible) or last pane
                if tree_visible {
                    ws.focus_target = FocusTarget::FileTree;
                } else {
                    ws.focus_target = FocusTarget::Pane;
                    if let Some(&last) = ids.last() {
                        ws.focused_pane_id = last;
                    }
                }
            }
            FocusTarget::Pane => {
                if let Some(idx) = ids.iter().position(|&id| id == ws.focused_pane_id) {
                    if idx > 0 {
                        ws.focused_pane_id = ids[idx - 1];
                    } else if preview_active {
                        ws.focus_target = FocusTarget::Preview;
                    } else if tree_visible {
                        ws.focus_target = FocusTarget::FileTree;
                    } else {
                        ws.focused_pane_id = ids[ids.len() - 1];
                    }
                }
            }
        }
        let new_id = self.ws().focused_pane_id;
        self.dismiss_done_on_focus(new_id);
    }

    /// Scroll a pane based on scrollbar click position.
    fn scroll_pane_to_click(&self, pane_id: usize, click_row: u16, inner: &Rect) {
        if let Some(pane) = self.ws().panes.get(&pane_id) {
            let (_, total_lines) = pane.scrollbar_info();
            let visible_rows = inner.height as usize;
            if total_lines <= visible_rows {
                return;
            }
            let max_scroll = total_lines.saturating_sub(visible_rows);
            // click_row relative to inner area: top = max scroll, bottom = 0
            let relative_y = click_row.saturating_sub(inner.y) as f32;
            let ratio = relative_y / inner.height.max(1) as f32;
            let target_scroll = ((1.0 - ratio) * max_scroll as f32) as usize;
            let mut parser = pane.parser.lock().unwrap_or_else(|e| e.into_inner());
            parser.screen_mut().set_scrollback(target_scroll);
        }
    }


    fn is_on_file_tree_border(&self, col: u16) -> bool {
        if let Some(rect) = self.ws().last_file_tree_rect {
            let border_col = rect.x + rect.width;
            col >= border_col.saturating_sub(1) && col <= border_col
        } else {
            false
        }
    }

    fn is_on_preview_border(&self, col: u16) -> bool {
        if let Some(rect) = self.ws().last_preview_rect {
            // When swapped: [tree][preview][panes] → drag the RIGHT edge of preview
            // When normal:  [tree][panes][preview] → drag the LEFT edge of preview
            let border_col = if self.layout_swapped {
                rect.x + rect.width
            } else {
                rect.x
            };
            col >= border_col.saturating_sub(1) && col <= border_col
        } else {
            false
        }
    }


    fn enter_copy_mode(&mut self) {
        let pane_id = self.ws().focused_pane_id;
        let rect = self.ws().last_pane_rects.iter()
            .find(|&&(id, _)| id == pane_id)
            .map(|&(_, r)| r);

        enum SizeSource { Rect, Parser, Default }

        let (screen_rows, screen_cols, source) = if let Some(rect) = rect {
            (rect.height.saturating_sub(2), rect.width.saturating_sub(2), SizeSource::Rect)
        } else if let Some(pane) = self.ws().panes.get(&pane_id) {
            let parser = pane.parser.lock().unwrap_or_else(|e| e.into_inner());
            let rows = parser.screen().size().0;
            let cols = parser.screen().size().1;
            (rows, cols, SizeSource::Parser)
        } else {
            (24u16, 80u16, SizeSource::Default)
        };

        match source {
            SizeSource::Parser => {
                self.status_flash = Some((
                    "copy mode: layout not determined, using parser size".to_string(),
                    std::time::Instant::now(),
                ));
            }
            SizeSource::Default => {
                self.status_flash = Some((
                    "copy mode: using default size (24x80)".to_string(),
                    std::time::Instant::now(),
                ));
            }
            SizeSource::Rect => {}
        }

        self.copy_mode = Some(CopyModeState {
            pane_id,
            cursor_row: screen_rows.saturating_sub(1),
            cursor_col: 0,
            selection_start: None,
            line_wise: false,
            screen_rows,
            screen_cols,
            first_g: false,
            scrollback_offset: 0,
        });
        self.dirty = true;
    }

    fn handle_copy_mode_key(&mut self, key: KeyEvent) -> Result<bool> {
        if self.copy_mode.is_none() {
            return Ok(false);
        }

        let pane_id = self.copy_mode.as_ref().unwrap().pane_id;
        let max_scrollback = self.ws().panes.get(&pane_id)
            .map(|p| p.total_scrollback.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(0);
        let action = Self::move_copy_cursor(self.copy_mode.as_mut().unwrap(), key, max_scrollback);

        match action {
            CopyModeAction::Quit => {
                self.copy_mode = None;
            }
            CopyModeAction::Yank => {
                self.yank_selection();
                self.copy_mode = None;
            }
            CopyModeAction::Continue => {}
        }
        self.dirty = true;
        Ok(true)
    }

    fn move_copy_cursor(cm: &mut CopyModeState, key: KeyEvent, max_scrollback: usize) -> CopyModeAction {
        if cm.screen_rows == 0 {
            return CopyModeAction::Continue;
        }

        let is_g = matches!(key.code, KeyCode::Char('g')) && key.modifiers == KeyModifiers::NONE;
        if !is_g {
            cm.first_g = false;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return CopyModeAction::Quit,
            KeyCode::Char('h') | KeyCode::Left => {
                cm.cursor_col = cm.cursor_col.saturating_sub(1);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                cm.cursor_col = (cm.cursor_col + 1).min(cm.screen_cols.saturating_sub(1));
            }
            KeyCode::Char('j') | KeyCode::Down if key.modifiers == KeyModifiers::NONE => {
                if cm.cursor_row >= cm.screen_rows.saturating_sub(1) && cm.scrollback_offset > 0 {
                    cm.scrollback_offset -= 1;
                } else {
                    cm.cursor_row = (cm.cursor_row + 1).min(cm.screen_rows.saturating_sub(1));
                }
            }
            KeyCode::Char('k') | KeyCode::Up if key.modifiers == KeyModifiers::NONE => {
                if cm.cursor_row == 0 && cm.scrollback_offset < max_scrollback {
                    cm.scrollback_offset += 1;
                } else {
                    cm.cursor_row = cm.cursor_row.saturating_sub(1);
                }
            }
            KeyCode::Char('0') => cm.cursor_col = 0,
            KeyCode::Char('$') => cm.cursor_col = cm.screen_cols.saturating_sub(1),
            KeyCode::Char('g') if key.modifiers == KeyModifiers::NONE => {
                if cm.first_g {
                    cm.cursor_row = 0;
                    cm.first_g = false;
                } else {
                    cm.first_g = true;
                }
            }
            KeyCode::Char('G') => {
                cm.cursor_row = cm.screen_rows.saturating_sub(1);
            }
            KeyCode::Char('v') if key.modifiers == KeyModifiers::NONE => {
                if cm.selection_start.is_some() && !cm.line_wise {
                    cm.selection_start = None;
                } else {
                    cm.selection_start = Some((cm.cursor_row, cm.cursor_col));
                    cm.line_wise = false;
                }
            }
            KeyCode::Char('V') => {
                if cm.selection_start.is_some() && cm.line_wise {
                    cm.selection_start = None;
                    cm.line_wise = false;
                } else {
                    cm.selection_start = Some((cm.cursor_row, 0));
                    cm.line_wise = true;
                }
            }
            KeyCode::Char('y') | KeyCode::Enter => return CopyModeAction::Yank,
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = cm.screen_rows / 2;
                if cm.cursor_row < half && cm.scrollback_offset < max_scrollback {
                    let remaining = half - cm.cursor_row;
                    cm.scrollback_offset = (cm.scrollback_offset + remaining as usize).min(max_scrollback);
                    cm.cursor_row = 0;
                } else {
                    cm.cursor_row = cm.cursor_row.saturating_sub(half);
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = cm.screen_rows / 2;
                let bottom = cm.screen_rows.saturating_sub(1);
                if cm.cursor_row + half > bottom && cm.scrollback_offset > 0 {
                    let overflow = (cm.cursor_row + half) - bottom;
                    cm.scrollback_offset = cm.scrollback_offset.saturating_sub(overflow as usize);
                    cm.cursor_row = bottom;
                } else {
                    cm.cursor_row = (cm.cursor_row + half).min(bottom);
                }
            }
            _ => {}
        }
        CopyModeAction::Continue
    }

    fn yank_selection(&mut self) {
        let cm = match self.copy_mode.as_ref() {
            Some(cm) => cm.clone(),
            None => return,
        };

        let pane_id = cm.pane_id;
        let parser_arc = match self.ws().panes.get(&pane_id) {
            Some(p) => std::sync::Arc::clone(&p.parser),
            None => return,
        };
        let text = {
            let mut parser = match parser_arc.lock() {
                Ok(guard) => guard,
                Err(_poisoned) => {
                    self.status_flash = Some((
                        "warning: terminal state may be corrupted".to_string(),
                        std::time::Instant::now(),
                    ));
                    return;
                }
            };

            let original_scrollback = parser.screen().scrollback();
            parser.screen_mut().set_scrollback(cm.scrollback_offset);
            let screen = parser.screen();

            let (start_row, start_col, end_row, end_col) = if let Some((sr, sc)) = cm.selection_start {
                let min_r = sr.min(cm.cursor_row);
                let max_r = sr.max(cm.cursor_row);
                if cm.line_wise {
                    (min_r, 0u16, max_r, cm.screen_cols)
                } else {
                    let (sc_norm, ec_norm) = if sr <= cm.cursor_row {
                        (sc, cm.cursor_col)
                    } else {
                        (cm.cursor_col, sc)
                    };
                    let end_col = ec_norm.saturating_add(1).min(cm.screen_cols);
                    (min_r, sc_norm, max_r, end_col)
                }
            } else {
                (cm.cursor_row, 0, cm.cursor_row, cm.screen_cols)
            };

            let mut lines = Vec::new();
            for row in start_row..=end_row {
                let col_start = if !cm.line_wise && row == start_row { start_col } else { 0 };
                let col_end = if !cm.line_wise && row == end_row { end_col } else { cm.screen_cols };
                let mut line = String::new();
                for col in col_start..col_end {
                    if let Some(cell) = screen.cell(row, col) {
                        let contents = cell.contents();
                        if contents.is_empty() {
                            line.push(' ');
                        } else {
                            line.push_str(contents);
                        }
                    }
                }
                lines.push(line.trim_end().to_string());
            }

            parser.screen_mut().set_scrollback(original_scrollback);

            lines.join("\n")
        };

        if !text.is_empty() {
            self.copy_to_clipboard(&text);
            let line_count = text.lines().count();
            self.status_flash = Some((
                format!("Copied ({} lines)", line_count),
                std::time::Instant::now(),
            ));
        }
    }


    fn open_pane_list_overlay(&mut self) {
        let pane_ids = self.ws().layout.collect_pane_ids();
        let focused = self.ws().focused_pane_id;
        let selected = pane_ids.iter().position(|&id| id == focused).unwrap_or(0);
        self.pane_list_overlay = PaneListOverlay {
            visible: true,
            selected,
            pane_ids,
        };
        self.dirty = true;
    }

    fn handle_pane_list_key(&mut self, key: KeyEvent) -> Result<bool> {
        let len = self.pane_list_overlay.pane_ids.len();
        if len == 0 {
            self.pane_list_overlay.visible = false;
            return Ok(true);
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.pane_list_overlay.selected = (self.pane_list_overlay.selected + 1) % len;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.pane_list_overlay.selected = (self.pane_list_overlay.selected + len - 1) % len;
            }
            KeyCode::Char(c @ '0'..='9') => {
                let digit = (c as usize) - ('0' as usize);
                self.pane_list_overlay.selected = digit.min(len.saturating_sub(1));
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(&selected_id) = self.pane_list_overlay.pane_ids.get(self.pane_list_overlay.selected) {
                    self.ws_mut().focused_pane_id = selected_id;
                }
                self.pane_list_overlay.visible = false;
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.pane_list_overlay.visible = false;
            }
            _ => {}
        }
        self.dirty = true;
        Ok(true)
    }


    fn sanitize_shell_arg(s: &str) -> Option<String> {
        if s.bytes().any(|b| b < 0x20 || b == 0x7f) {
            return None;
        }
        Some(s.replace('\'', "'\\''"))
    }

    fn open_in_editor(&mut self, path: &std::path::Path) {
        let editor = self.config.filetree.editor.trim().to_string();
        if editor.is_empty() {
            self.status_flash = Some((
                "no editor configured".to_string(),
                std::time::Instant::now(),
            ));
            return;
        }
        let escaped_editor = match Self::sanitize_shell_arg(&editor) {
            Some(e) => e,
            None => {
                self.status_flash = Some((
                    "editor name contains invalid characters".to_string(),
                    std::time::Instant::now(),
                ));
                return;
            }
        };
        let path_str = path.to_string_lossy();
        let escaped_path = match Self::sanitize_shell_arg(&path_str) {
            Some(p) => p,
            None => {
                self.status_flash = Some((
                    "file path contains invalid characters".to_string(),
                    std::time::Instant::now(),
                ));
                return;
            }
        };
        let cmd = format!("'{}' '{}'\n", escaped_editor, escaped_path);

        let pane_id = self.ws().focused_pane_id;
        if let Some(pane) = self.ws_mut().panes.get_mut(&pane_id) {
            let _ = pane.write_input(cmd.as_bytes());
        }
        self.ws_mut().focus_target = FocusTarget::Pane;
        self.dirty = true;
    }

    fn handle_filetree_action_popup_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Tab => {
                self.filetree_action_popup.selected = (self.filetree_action_popup.selected + 1) % FILETREE_ACTION_COUNT;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.filetree_action_popup.selected = (self.filetree_action_popup.selected + FILETREE_ACTION_COUNT - 1) % FILETREE_ACTION_COUNT;
            }
            KeyCode::Enter => {
                let path = self.filetree_action_popup.file_path.clone();
                if self.filetree_action_popup.selected == 0 {
                    self.clear_selection_if_preview();
                    let mut picker = self.image_picker.take();
                    self.ws_mut().preview.load(&path, picker.as_mut());
                    self.image_picker = picker;
                } else {
                    self.open_in_editor(&path);
                }
                self.filetree_action_popup.visible = false;
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.filetree_action_popup.visible = false;
            }
            _ => {}
        }
        self.dirty = true;
        Ok(true)
    }

    pub fn handle_mouse_event(&mut self, mouse: MouseEvent) {
        // Cancel any in-progress rename on mouse click so
        // the buffer can't silently migrate to another tab.
        if matches!(mouse.kind, MouseEventKind::Down(_)) && self.rename_input.is_some() {
            let needs_relayout = !self.status_bar_visible;
            self.rename_input = None;
            self.dirty = true;
            if needs_relayout { self.mark_layout_change(); }
        }

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let col = mouse.column;
                let row = mouse.row;

                // Clear previous selection on any click
                self.selection = None;

                // Check tab bar clicks
                for &(tab_idx, rect) in &self.last_tab_rects {
                    if col >= rect.x && col < rect.x + rect.width
                        && row >= rect.y && row < rect.y + rect.height
                    {
                        let now = Instant::now();
                        let is_double = matches!(
                            self.last_tab_click,
                            Some((prev_idx, prev_t))
                                if prev_idx == tab_idx
                                    && now.duration_since(prev_t).as_millis() < 500
                        );
                        self.active_tab = tab_idx;
                        if is_double {
                            self.rename_input = Some(String::new());
                            self.last_tab_click = None;
                        } else {
                            self.last_tab_click = Some((tab_idx, now));
                        }
                        self.dirty = true;
                        return;
                    }
                }
                // Click missed the tab bar — reset double-click tracker.
                self.last_tab_click = None;

                // Check [+] new tab button
                if let Some(rect) = self.last_new_tab_rect {
                    if col >= rect.x && col < rect.x + rect.width
                        && row >= rect.y && row < rect.y + rect.height
                    {
                        let _ = self.new_tab();
                        return;
                    }
                }

                // Check border drag (file tree / preview)
                if self.is_on_file_tree_border(col) {
                    self.dragging = Some(DragTarget::FileTreeBorder);
                    return;
                }
                if self.is_on_preview_border(col) {
                    self.dragging = Some(DragTarget::PreviewBorder);
                    return;
                }

                // Check pane split border drag
                if let Some(pane_area) = self.ws().last_pane_rects.first().map(|_| {
                    // Compute the total pane area from all pane rects
                    let rects = &self.ws().last_pane_rects;
                    let min_x = rects.iter().map(|(_, r)| r.x).min().unwrap_or(0);
                    let min_y = rects.iter().map(|(_, r)| r.y).min().unwrap_or(0);
                    let max_x = rects.iter().map(|(_, r)| r.x + r.width).max().unwrap_or(0);
                    let max_y = rects.iter().map(|(_, r)| r.y + r.height).max().unwrap_or(0);
                    Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
                }) {
                    let boundaries = self.ws().layout.split_boundaries(pane_area);
                    for (boundary, direction, path) in boundaries {
                        let on_border = match direction {
                            SplitDirection::Vertical => {
                                col >= boundary.saturating_sub(1) && col <= boundary
                                    && row >= pane_area.y && row < pane_area.y + pane_area.height
                            }
                            SplitDirection::Horizontal => {
                                row >= boundary.saturating_sub(1) && row <= boundary
                                    && col >= pane_area.x && col < pane_area.x + pane_area.width
                            }
                        };
                        if on_border {
                            self.dragging = Some(DragTarget::PaneSplit(path, direction, pane_area));
                            return;
                        }
                    }
                }

                // Check file tree click
                if let Some(rect) = self.ws().last_file_tree_rect {
                    if col >= rect.x && col < rect.x + rect.width
                        && row >= rect.y && row < rect.y + rect.height
                    {
                        self.ws_mut().focus_target = FocusTarget::FileTree;
                        let inner_y = row.saturating_sub(rect.y + 1);
                        let scroll = self.ws().file_tree.scroll_offset;
                        let entry_idx = scroll + inner_y as usize;
                        let entry_count = self.ws().file_tree.visible_entries().len();
                        if entry_idx < entry_count {
                            self.ws_mut().file_tree.selected_index = entry_idx;
                            let path = self.ws_mut().file_tree.toggle_or_select();
                            if let Some(path) = path {
                                self.clear_selection_if_preview();
                                let mut picker = self.image_picker.take();
                                self.ws_mut().preview.load(&path, picker.as_mut());
                                self.image_picker = picker;
                            }
                        }
                        return;
                    }
                }

                // Check preview click
                if let Some(rect) = self.ws().last_preview_rect {
                    if col >= rect.x && col < rect.x + rect.width
                        && row >= rect.y && row < rect.y + rect.height
                    {
                        self.ws_mut().focus_target = FocusTarget::Preview;
                        return;
                    }
                }

                // Check pane clicks
                let pane_rects = self.ws().last_pane_rects.clone();
                for (pane_id, rect) in pane_rects {
                    if col >= rect.x && col < rect.x + rect.width
                        && row >= rect.y && row < rect.y + rect.height
                    {
                        self.ws_mut().focused_pane_id = pane_id;
                        self.ws_mut().focus_target = FocusTarget::Pane;
                        self.dismiss_done_on_focus(pane_id);

                        // Check if clicking on scrollbar (rightmost column inside border)
                        let scrollbar_col = rect.x + rect.width - 2; // -1 border, -1 scrollbar
                        if col >= scrollbar_col {
                            let inner = Rect::new(rect.x + 1, rect.y + 1, rect.width.saturating_sub(2), rect.height.saturating_sub(2));
                            self.scroll_pane_to_click(pane_id, row, &inner);
                            self.dragging = Some(DragTarget::Scrollbar(pane_id, inner));
                        }
                        return;
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let col = mouse.column;
                let row = mouse.row;

                // Border drag takes priority
                if let Some(ref target) = self.dragging.clone() {
                    match target {
                        DragTarget::FileTreeBorder => {
                            self.file_tree_width = col.clamp(10, 60);
                        }
                        DragTarget::PreviewBorder => {
                            if let Some(rect) = self.ws().last_preview_rect {
                                if self.layout_swapped {
                                    let new_width = col.saturating_sub(rect.x).clamp(15, 80);
                                    self.preview_width = new_width;
                                } else {
                                    let total_right = rect.x + rect.width;
                                    let new_width = total_right.saturating_sub(col).clamp(15, 80);
                                    self.preview_width = new_width;
                                }
                            }
                        }
                        DragTarget::PaneSplit(path, direction, area) => {
                            let new_ratio = match direction {
                                SplitDirection::Vertical => {
                                    (col.saturating_sub(area.x) as f32) / area.width.max(1) as f32
                                }
                                SplitDirection::Horizontal => {
                                    (row.saturating_sub(area.y) as f32) / area.height.max(1) as f32
                                }
                            };
                            self.ws_mut().layout.update_ratio(path, new_ratio);
                        }
                        DragTarget::Scrollbar(pane_id, inner) => {
                            self.scroll_pane_to_click(*pane_id, row, inner);
                        }
                    }
                    return;
                }

                // Text selection: extend if active, or start new
                if let Some(ref mut sel) = self.selection {
                    let inner = sel.content_rect;
                    match sel.target {
                        SelectionTarget::Pane(_) => {
                            // Pane: screen-relative coords inside inner.
                            sel.end_col = col
                                .saturating_sub(inner.x)
                                .min(inner.width.saturating_sub(1)) as u32;
                            sel.end_row = row
                                .saturating_sub(inner.y)
                                .min(inner.height.saturating_sub(1)) as u32;
                        }
                        SelectionTarget::Preview => {
                            // Preview: translate screen coords to
                            // source (absolute line + char offset)
                            // using the current scroll state.
                            let scroll_v = self.ws().preview.scroll_offset;
                            let h_scroll = self.ws().preview.h_scroll_offset;

                            let mut screen_col = col.saturating_sub(inner.x);
                            let mut screen_row = row.saturating_sub(inner.y);

                            // Auto-scroll when drag reaches an edge.
                            // Move the underlying scroll by one step
                            // so the cursor can "pull" more content
                            // into view. Clamp screen position so the
                            // computed source coord tracks the new edge.
                            if col < inner.x {
                                self.ws_mut().preview.scroll_left(2);
                                screen_col = 0;
                            } else if col >= inner.x + inner.width {
                                self.ws_mut().preview.scroll_right(2);
                                screen_col = inner.width.saturating_sub(1);
                            }
                            if row < inner.y {
                                self.ws_mut().preview.scroll_up(1);
                                screen_row = 0;
                            } else if row >= inner.y + inner.height {
                                self.ws_mut().preview.scroll_down(1);
                                screen_row = inner.height.saturating_sub(1);
                            }

                            // Re-read scroll state in case we changed it above.
                            let scroll_v = self.ws().preview.scroll_offset.max(scroll_v);
                            let h_scroll = self.ws().preview.h_scroll_offset.max(h_scroll);
                            // Clamp end_row to a valid absolute line index.
                            let lines_len = self.ws().preview.lines.len();
                            let abs_row = (scroll_v + screen_row as usize)
                                .min(lines_len.saturating_sub(1));
                            let abs_col = screen_col as usize + h_scroll;
                            // Update the selection endpoint (source coords).
                            if let Some(sel) = self.selection.as_mut() {
                                sel.end_row = abs_row as u32;
                                sel.end_col = abs_col as u32;
                            }
                        }
                    }
                } else {
                    // Start new selection — try pane areas first, then preview
                    let pane_rects = self.ws().last_pane_rects.clone();
                    let mut started = false;
                    for (pane_id, rect) in pane_rects {
                        if col >= rect.x && col < rect.x + rect.width
                            && row >= rect.y && row < rect.y + rect.height
                        {
                            let inner = Rect::new(
                                rect.x + 1, rect.y + 1,
                                rect.width.saturating_sub(2),
                                rect.height.saturating_sub(2),
                            );
                            let cell_col = col.saturating_sub(inner.x) as u32;
                            let cell_row = row.saturating_sub(inner.y) as u32;
                            self.selection = Some(TextSelection {
                                target: SelectionTarget::Pane(pane_id),
                                start_row: cell_row,
                                start_col: cell_col,
                                end_row: cell_row,
                                end_col: cell_col,
                                content_rect: inner,
                            });
                            started = true;
                            break;
                        }
                    }
                    // Preview drag selection. Content area is the inside
                    // of the preview border minus the 5-column line-number
                    // gutter (format "{:>4}│"). Selection stores source
                    // coords (abs line index, char offset) so it can
                    // survive scrolling.
                    if !started {
                        if let Some(rect) = self.ws().last_preview_rect {
                            if col >= rect.x && col < rect.x + rect.width
                                && row >= rect.y && row < rect.y + rect.height
                            {
                                const GUTTER: u16 = 5;
                                let inner = Rect::new(
                                    rect.x + 1 + GUTTER, rect.y + 1,
                                    rect.width.saturating_sub(2 + GUTTER),
                                    rect.height.saturating_sub(2),
                                );
                                // Ignore drags that start inside the gutter
                                if col >= inner.x && row >= inner.y {
                                    let screen_col = col.saturating_sub(inner.x);
                                    let screen_row = row.saturating_sub(inner.y);
                                    let scroll_v = self.ws().preview.scroll_offset;
                                    let h_scroll = self.ws().preview.h_scroll_offset;
                                    let lines_len = self.ws().preview.lines.len();
                                    let abs_row = (scroll_v + screen_row as usize)
                                        .min(lines_len.saturating_sub(1));
                                    let abs_col = screen_col as usize + h_scroll;
                                    self.selection = Some(TextSelection {
                                        target: SelectionTarget::Preview,
                                        start_row: abs_row as u32,
                                        start_col: abs_col as u32,
                                        end_row: abs_row as u32,
                                        end_col: abs_col as u32,
                                        content_rect: inner,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.dragging = None;

                // Copy selected text to clipboard
                if let Some(sel) = self.selection.clone() {
                    let (sr, sc, er, ec) = sel.normalized();
                    if sr != er || sc != ec {
                        let text = match sel.target {
                            SelectionTarget::Pane(pane_id) => self
                                .ws()
                                .panes
                                .get(&pane_id)
                                .map(|p| extract_selected_text(p, sr, sc, er, ec))
                                .unwrap_or_default(),
                            SelectionTarget::Preview => extract_preview_selected_text(
                                &self.ws().preview,
                                sr,
                                sc,
                                er,
                                ec,
                            ),
                        };
                        if !text.is_empty() {
                            self.copy_to_clipboard(&text);
                        }
                    }
                    // Keep selection visible until next click
                }
            }
            MouseEventKind::ScrollUp => {
                let col = mouse.column;
                let row = mouse.row;

                if let Some(rect) = self.ws().last_file_tree_rect {
                    if col >= rect.x && col < rect.x + rect.width
                        && row >= rect.y && row < rect.y + rect.height
                    {
                        self.ws_mut().file_tree.scroll_up(3);
                        return;
                    }
                }
                if let Some(rect) = self.ws().last_preview_rect {
                    if col >= rect.x && col < rect.x + rect.width
                        && row >= rect.y && row < rect.y + rect.height
                    {
                        self.ws_mut().preview.scroll_up(3);
                        return;
                    }
                }
                let pane_rects = self.ws().last_pane_rects.clone();
                for (pane_id, rect) in pane_rects {
                    if col >= rect.x && col < rect.x + rect.width
                        && row >= rect.y && row < rect.y + rect.height
                    {
                        if let Some(pane) = self.ws().panes.get(&pane_id) {
                            pane.scroll_up(3);
                        }
                        return;
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                let col = mouse.column;
                let row = mouse.row;

                if let Some(rect) = self.ws().last_file_tree_rect {
                    if col >= rect.x && col < rect.x + rect.width
                        && row >= rect.y && row < rect.y + rect.height
                    {
                        self.ws_mut().file_tree.scroll_down(3);
                        return;
                    }
                }
                if let Some(rect) = self.ws().last_preview_rect {
                    if col >= rect.x && col < rect.x + rect.width
                        && row >= rect.y && row < rect.y + rect.height
                    {
                        self.ws_mut().preview.scroll_down(3);
                        return;
                    }
                }
                let pane_rects = self.ws().last_pane_rects.clone();
                for (pane_id, rect) in pane_rects {
                    if col >= rect.x && col < rect.x + rect.width
                        && row >= rect.y && row < rect.y + rect.height
                    {
                        if let Some(pane) = self.ws().panes.get(&pane_id) {
                            pane.scroll_down(3);
                        }
                        return;
                    }
                }
            }
            MouseEventKind::ScrollLeft => {
                let col = mouse.column;
                let row = mouse.row;
                if let Some(rect) = self.ws().last_preview_rect {
                    if col >= rect.x && col < rect.x + rect.width
                        && row >= rect.y && row < rect.y + rect.height
                    {
                        self.ws_mut().preview.scroll_left(4);
                    }
                }
            }
            MouseEventKind::ScrollRight => {
                let col = mouse.column;
                let row = mouse.row;
                if let Some(rect) = self.ws().last_preview_rect {
                    if col >= rect.x && col < rect.x + rect.width
                        && row >= rect.y && row < rect.y + rect.height
                    {
                        self.ws_mut().preview.scroll_right(4);
                    }
                }
            }
            MouseEventKind::Moved => {
                let col = mouse.column;
                let old_hover = self.hover_border.clone();
                if self.is_on_file_tree_border(col) {
                    self.hover_border = Some(DragTarget::FileTreeBorder);
                } else if self.is_on_preview_border(col) {
                    self.hover_border = Some(DragTarget::PreviewBorder);
                } else {
                    self.hover_border = None;
                }
                if self.hover_border != old_hover {
                    self.dirty = true;
                }
            }
            _ => {}
        }
    }


    /// Forward pasted text to PTY, wrapping in bracketed paste only if
    /// the PTY application has enabled the mode (e.g. Claude Code, modern
    /// readline). Sending bracketed paste to a shell that hasn't opted in
    /// causes the escape sequences to appear as literal text (issue #2).
    pub fn forward_paste_to_pty(&mut self, text: &str) -> Result<()> {
        let focused_id = self.ws().focused_pane_id;
        if let Some(pane) = self.ws_mut().panes.get_mut(&focused_id) {
            pane.scroll_reset();
            if pane.is_bracketed_paste_enabled() {
                let mut data = Vec::with_capacity(text.len() + 12);
                data.extend_from_slice(b"\x1b[200~");
                data.extend_from_slice(text.as_bytes());
                data.extend_from_slice(b"\x1b[201~");
                pane.write_input(&data)?;
            } else {
                pane.write_input(text.as_bytes())?;
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn forward_key_to_pty(&mut self, key: KeyEvent) -> Result<()> {
        let focused_id = self.ws().focused_pane_id;
        if let Some(pane) = self.ws_mut().panes.get_mut(&focused_id) {
            pane.scroll_reset();
            if let Some(bytes) = key_event_to_bytes(&key) {
                pane.write_input(&bytes)?;
            }
        }
        Ok(())
    }

    pub fn drain_pty_events(&mut self) -> bool {
        let mut had_events = false;
        while let Ok(event) = self.event_rx.try_recv() {
            had_events = true;
            match event {
                AppEvent::PtyEof(pane_id) => {
                    let worktree_path = self.find_pane(pane_id)
                        .and_then(|p| p.worktree_path.clone());
                    let branch = self.find_pane(pane_id)
                        .and_then(|p| p.branch_name.clone());

                    for ws in &mut self.workspaces {
                        if let Some(pane) = ws.panes.get_mut(&pane_id) {
                            pane.exited = true;
                            break;
                        }
                    }

                    if let (Some(wt_path), Some(_branch)) = (worktree_path, branch) {
                        let close_worktree = self.config.worktree.close_worktree.clone();
                        if close_worktree != "never" {
                            if let Some(handle) = &self.tokio_handle {
                                let tx = self.event_tx.clone();
                                let main_branch = self.config.worktree.main_branch.clone();
                                handle.spawn(async move {
                                    let wt = wt_path.clone();
                                    let merged = tokio::task::spawn_blocking(move || {
                                        crate::worktree::WorktreeManager::new()
                                            .check_merged(&wt, &main_branch)
                                    }).await.unwrap_or(false);
                                    if merged {
                                        let _ = tx.send(AppEvent::WorktreeMerged { worktree_path: wt_path });
                                    }
                                });
                            }
                        }
                    }
                }
                AppEvent::CwdChanged(pane_id, new_cwd) => {
                    // Security: resolve symlinks and relative components.
                    // Reject paths that don't resolve to a real directory
                    // (prevents OSC 7 escape sequence path injection).
                    let new_cwd = match new_cwd.canonicalize() {
                        Ok(p) if p.is_dir() => p,
                        _ => continue,
                    };
                    let mut preview_closed_by_cwd = false;
                    for ws in &mut self.workspaces {
                        if ws.panes.contains_key(&pane_id) {
                            // Update pane's cwd
                            if let Some(pane) = ws.panes.get_mut(&pane_id) {
                                pane.cwd = new_cwd.clone();
                            }
                            if ws.focused_pane_id == pane_id {
                                let prev_show_hidden = ws.file_tree.show_hidden;
                                ws.file_tree = FileTree::new(new_cwd.clone());
                                // FileTree::new defaults to show_hidden=true
                                // Only toggle if the previous state was different
                                if ws.file_tree.show_hidden != prev_show_hidden {
                                    ws.file_tree.toggle_hidden();
                                }
                                ws.cwd = new_cwd;
                                ws.name = dir_name(&ws.cwd);
                                ws.preview.close();
                                preview_closed_by_cwd = true;
                            }
                            break;
                        }
                    }
                    if preview_closed_by_cwd {
                        self.preview_zoomed = false;
                    }
                }
                AppEvent::PtyOutput { pane_id, lines } => {
                    // Accumulate meaningful lines into the ring buffer.
                    // Filter out Claude Code UI noise lines (bypass/status bars).
                    if self.ai_title_enabled && !lines.is_empty() {
                        let mut shell_prompt_seen = false;
                        {
                            let ring = self.pane_output_rings.entry(pane_id).or_default();
                            for line in &lines {
                                if ai_title::is_noise_line(line) {
                                    continue;
                                }
                                if ai_title::is_shell_prompt(line) {
                                    shell_prompt_seen = true;
                                    continue; // don't add the prompt line itself to the ring
                                }
                                ring.push_back(line.clone());
                                if ring.len() > 100 {
                                    ring.pop_front();
                                }
                            }
                        }
                        // Fallback trigger: shell prompt detected (for non-Claude panes
                        // that don't send HookEvent::Stop)
                        if shell_prompt_seen {
                            let interval = self.config.ai_title_engine.update_interval_sec;
                            let should_request = !self.ai_title_in_flight.contains(&pane_id)
                                && self.last_ai_title_request
                                    .get(&pane_id)
                                    .map(|t| t.elapsed().as_secs() >= interval)
                                    .unwrap_or(true);
                            if should_request {
                                if let Some(ring) = self.pane_output_rings.get(&pane_id) {
                                    if !ring.is_empty() {
                                        let output = ring.iter().cloned().collect::<Vec<_>>().join("\n");
                                        let tx = self.event_tx.clone();
                                        if let Some(handle) = &self.tokio_handle {
                                            self.last_ai_title_request.insert(pane_id, Instant::now());
                                            self.ai_title_in_flight.insert(pane_id);
                                            let config = self.config.ai_title_engine.clone();
                                            let prompt_template = self.config.ai.title.prompt.clone();
                                            let ollama_url = self.config.ai.ollama.base_url.clone();
                                            let ollama_model = self.config.ai.ollama.model.clone();
                                            let gemini_api_key = self.config.ai.gemini.api_key.clone();
                                            let gemini_model = self.config.ai.gemini.model.clone();
                                            handle.spawn(async move {
                                                let title = ai_title::generate_title(&output, &config, &prompt_template, &ollama_url, &ollama_model, &gemini_api_key, &gemini_model).await;
                                                let _ = tx.send(AppEvent::AiTitleGenerated { pane_id, title });
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                    self.dirty = true;
                }
                AppEvent::HookReceived { pane_id, event } => {
                    let pane_exists = self.workspaces.iter()
                        .any(|ws| ws.panes.contains_key(&pane_id));
                    if pane_exists {
                        let state = self.pane_states.entry(pane_id).or_default();
                        match event {
                            HookEvent::Stop => {
                                state.status = PaneStatus::Done;
                                state.dismissed = false;
                                // Claude Code returned to prompt — good time to generate title
                                if self.ai_title_enabled {
                                    let interval = self.config.ai_title_engine.update_interval_sec;
                                    let should_request = !self.ai_title_in_flight.contains(&pane_id)
                                        && self.last_ai_title_request
                                            .get(&pane_id)
                                            .map(|t| t.elapsed().as_secs() >= interval)
                                            .unwrap_or(true);
                                    if should_request {
                                        if let Some(ring) = self.pane_output_rings.get(&pane_id) {
                                            if !ring.is_empty() {
                                                let output = ring.iter().cloned().collect::<Vec<_>>().join("\n");
                                                let tx = self.event_tx.clone();
                                                if let Some(handle) = &self.tokio_handle {
                                                    self.last_ai_title_request.insert(pane_id, Instant::now());
                                                    self.ai_title_in_flight.insert(pane_id);
                                                    let config = self.config.ai_title_engine.clone();
                                                    let prompt_template = self.config.ai.title.prompt.clone();
                                                    let ollama_url = self.config.ai.ollama.base_url.clone();
                                                    let ollama_model = self.config.ai.ollama.model.clone();
                                                    let gemini_api_key = self.config.ai.gemini.api_key.clone();
                                                    let gemini_model = self.config.ai.gemini.model.clone();
                                                    handle.spawn(async move {
                                                        let title = ai_title::generate_title(&output, &config, &prompt_template, &ollama_url, &ollama_model, &gemini_api_key, &gemini_model).await;
                                                        let _ = tx.send(AppEvent::AiTitleGenerated { pane_id, title });
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            HookEvent::UserPromptSubmit | HookEvent::PreToolUse => {
                                state.status = PaneStatus::Running;
                                state.dismissed = false;
                            }
                            HookEvent::Notification => {
                                state.status = PaneStatus::Waiting;
                                state.dismissed = false;
                            }
                        }
                    }
                }
                AppEvent::AiTitleGenerated { pane_id, title } => {
                    self.ai_title_in_flight.remove(&pane_id);
                    if let Some(t) = title {
                        self.ai_titles.insert(pane_id, t);
                    }
                }
                AppEvent::BranchNameGenerated { branch } => {
                    self.pane_create_dialog.generating_name = false;
                    if !branch.is_empty() {
                        self.pane_create_dialog.branch_name = branch;
                        self.pane_create_dialog.error_msg = None;
                    } else {
                        self.pane_create_dialog.error_msg =
                            Some("AI generation failed or timed out".to_string());
                    }
                }
                AppEvent::WorktreeCreated { pane_id, cwd, branch_name: _ } => {
                    for ws in &mut self.workspaces {
                        if let Some(pane) = ws.panes.get_mut(&pane_id) {
                            pane.worktree_path = Some(cwd.clone());
                            // Shell-quote the path to handle spaces and special characters
                            let path_str = cwd.to_string_lossy();
                            let quoted = format!("'{}'", path_str.replace('\'', "'\\''"));
                            let cd_cmd = format!("cd {}\n", quoted);
                            let _ = pane.write_input(cd_cmd.as_bytes());
                            // Launch pending agent command after cd
                            if let Some(agent_cmd) = pane.pending_agent.take() {
                                let cmd = format!("{}\n", agent_cmd);
                                let _ = pane.write_input(cmd.as_bytes());
                            }
                            break;
                        }
                    }
                    // Refresh worktree list asynchronously to avoid blocking the UI thread
                    let repo_root = self.ws().cwd.clone();
                    if let Some(handle) = &self.tokio_handle {
                        let tx = self.event_tx.clone();
                        handle.spawn(async move {
                            if let Ok(worktrees) = tokio::task::spawn_blocking(move || {
                                crate::worktree::WorktreeManager::new().list(&repo_root)
                            }).await.unwrap_or(Err(anyhow::anyhow!("task failed"))) {
                                let _ = tx.send(AppEvent::WorktreesListed { worktrees });
                            }
                        });
                    }
                }
                AppEvent::WorktreeCreateFailed { pane_id: _, branch_name: _, error } => {
                    eprintln!("glowmux: worktree create failed: {}", error);
                    // Surface error in the dialog if it's still open, otherwise show in status
                    if self.pane_create_dialog.visible {
                        self.pane_create_dialog.error_msg = Some(format!("Worktree error: {}", error));
                    }
                    self.dirty = true;
                }
                AppEvent::WorktreeMerged { worktree_path } => {
                    let close_worktree = self.config.worktree.close_worktree.clone();
                    match close_worktree.as_str() {
                        "auto" => {
                            let repo_root = self.ws().cwd.clone();
                            if let Some(handle) = &self.tokio_handle {
                                let path = worktree_path;
                                let root = repo_root;
                                handle.spawn(async move {
                                    let _ = tokio::task::spawn_blocking(move || {
                                        crate::worktree::WorktreeManager::new().remove(&path, &root)
                                    }).await;
                                });
                            }
                        }
                        "ask" => {
                            let branch = self.workspaces.iter()
                                .flat_map(|ws| ws.panes.values())
                                .find(|p| p.worktree_path.as_ref() == Some(&worktree_path))
                                .and_then(|p| p.branch_name.clone())
                                .unwrap_or_default();
                            self.worktree_cleanup_dialog = Some(WorktreeCleanupDialog {
                                visible: true,
                                worktree_path,
                                branch,
                                focused: CloseConfirmFocus::No,
                            });
                        }
                        _ => {} // "never"
                    }
                }
                AppEvent::WorktreesListed { worktrees } => {
                    self.ws_mut().worktrees = worktrees;
                }
            }
        }
        if had_events {
            self.dirty = true;
        }
        had_events
    }

    pub fn dismiss_done_on_focus(&mut self, pane_id: usize) {
        if let Some(state) = self.pane_states.get_mut(&pane_id) {
            if state.status == PaneStatus::Done {
                state.dismissed = true;
            }
        }
    }

    pub fn pane_status(&self, pane_id: usize) -> PaneStatus {
        let hook_status = self.pane_states
            .get(&pane_id)
            .map(|s| s.status)
            .unwrap_or(PaneStatus::Idle);

        let claude_state = self.claude_monitor.state(pane_id);

        // JSONL-derived activity always wins for Running — hooks are too coarse
        // to capture the sub-turn tool execution cycle.
        if claude_state.is_working {
            return PaneStatus::Running;
        }

        // Hook says Waiting (Notification) → trust it; JSONL has no Waiting signal.
        if hook_status == PaneStatus::Waiting {
            return PaneStatus::Waiting;
        }

        // JSONL has token data → Claude ran at least once this session.
        if claude_state.total_tokens() > 0 {
            return PaneStatus::Done;
        }

        // Hook-driven Done/Running as final fallback.
        if hook_status != PaneStatus::Idle {
            return hook_status;
        }

        PaneStatus::Idle
    }

    pub fn pane_state_dismissed(&self, pane_id: usize) -> bool {
        self.pane_states
            .get(&pane_id)
            .map(|s| s.dismissed)
            .unwrap_or(false)
    }

    fn find_pane(&self, pane_id: usize) -> Option<&crate::pane::Pane> {
        self.workspaces.iter()
            .flat_map(|ws| ws.panes.values())
            .find(|p| p.id == pane_id)
    }

    pub fn shutdown(&mut self) {
        for ws in &mut self.workspaces {
            ws.shutdown();
        }
    }
}

fn dir_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn restore_session_workspaces(
    app: &mut App,
    session: &crate::session::SessionData,
    pane_rows: u16,
    pane_cols: u16,
    tx: &std::sync::mpsc::Sender<AppEvent>,
) -> anyhow::Result<()> {
    if session.workspaces.is_empty() {
        return Err(anyhow::anyhow!("empty session"));
    }

    for ws in &mut app.workspaces {
        ws.shutdown();
    }
    app.workspaces.clear();

    for ws_snap in &session.workspaces {
        if ws_snap.panes.is_empty() {
            continue;
        }
        let first_pane = &ws_snap.panes[0];
        let pane_id = app.next_pane_id;
        app.next_pane_id += 1;

        let ws = Workspace::new(
            ws_snap.name.clone(),
            ws_snap.cwd.clone(),
            pane_id,
            pane_rows,
            pane_cols,
            tx.clone(),
        )?;
        app.workspaces.push(ws);

        if !first_pane.title.is_empty() {
            app.ai_titles.insert(pane_id, first_pane.title.clone());
        }

        let ws_idx = app.workspaces.len() - 1;
        for pane_snap in ws_snap.panes.iter().skip(1) {
            let new_pane_id = app.next_pane_id;
            app.next_pane_id += 1;
            let pane = crate::pane::Pane::new(new_pane_id, pane_rows, pane_cols, tx.clone())?;
            let ws = &mut app.workspaces[ws_idx];
            let focused = ws.focused_pane_id;
            ws.panes.insert(new_pane_id, pane);
            ws.layout.split_pane(focused, new_pane_id, SplitDirection::Vertical);
            if !pane_snap.title.is_empty() {
                app.ai_titles.insert(new_pane_id, pane_snap.title.clone());
            }
        }
    }

    if session.active_tab < app.workspaces.len() {
        app.active_tab = session.active_tab;
    }

    Ok(())
}

/// Extract text from a pane's vt100 screen within a selection range.
fn extract_selected_text(pane: &Pane, sr: u32, sc: u32, er: u32, ec: u32) -> String {
    let parser = pane.parser.lock().unwrap_or_else(|e| e.into_inner());
    let screen = parser.screen();
    let mut lines = Vec::new();

    for row in sr..=er {
        let mut line = String::new();
        let col_start = if row == sr { sc } else { 0 };
        let col_end = if row == er { ec } else { 999 };

        for col in col_start..=col_end {
            if let Some(cell) = screen.cell(row as u16, col as u16) {
                let contents = cell.contents();
                if contents.is_empty() {
                    line.push(' ');
                } else {
                    line.push_str(contents);
                }
            }
        }
        lines.push(line.trim_end().to_string());
    }

    // Remove trailing empty lines
    while lines.last().is_some_and(|l: &String| l.is_empty()) {
        lines.pop();
    }

    lines.join("\n")
}

/// Extract text from the file preview within a selection range.
/// `sr`/`er` are absolute line indices; `sc`/`ec` are char offsets
/// within the line (selection is stored in source coordinates so it
/// survives scrolling). Trailing empty lines are stripped.
fn extract_preview_selected_text(preview: &crate::preview::Preview, sr: u32, sc: u32, er: u32, ec: u32) -> String {
    let lines = &preview.lines;
    let mut out: Vec<String> = Vec::new();

    for abs_row in sr..=er {
        let idx = abs_row as usize;
        if idx >= lines.len() {
            break;
        }
        let line = &lines[idx];
        let chars: Vec<char> = line.chars().collect();

        let col_start = if abs_row == sr { sc as usize } else { 0 };
        let col_end_inclusive = if abs_row == er { ec as usize } else {
            chars.len().saturating_sub(1)
        };

        let start = col_start.min(chars.len());
        let end = (col_end_inclusive.saturating_add(1)).min(chars.len());
        let slice: String = if start < end {
            chars[start..end].iter().collect()
        } else {
            String::new()
        };
        out.push(slice);
    }

    // Strip trailing empty lines only.
    while out.last().is_some_and(|l: &String| l.is_empty()) {
        out.pop();
    }

    out.join("\n")
}

/// Public wrapper for key_event_to_bytes (used by main.rs paste detection).
pub fn key_event_to_bytes_pub(key: &KeyEvent) -> Option<Vec<u8>> {
    key_event_to_bytes(key)
}

/// Convert a crossterm KeyEvent into bytes suitable for PTY input.
fn key_event_to_bytes(key: &KeyEvent) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                let ctrl_byte = (c.to_ascii_lowercase() as u8).wrapping_sub(b'a').wrapping_add(1);
                if ctrl_byte <= 26 {
                    if alt {
                        // Alt+Ctrl+Char → ESC + ctrl byte
                        Some(vec![0x1b, ctrl_byte])
                    } else {
                        Some(vec![ctrl_byte])
                    }
                } else {
                    Some(c.to_string().into_bytes())
                }
            } else if alt {
                // Alt+Char → ESC + char (standard xterm behavior)
                let mut bytes = vec![0x1b];
                bytes.extend_from_slice(c.to_string().as_bytes());
                Some(bytes)
            } else {
                Some(c.to_string().into_bytes())
            }
        }
        // Alt+Enter → send newline (\n) for multi-line input in Claude Code
        KeyCode::Enter if alt => Some(vec![b'\n']),
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::BackTab => Some(b"\x1b[Z".to_vec()),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        KeyCode::F(n) => {
            let seq = match n {
                1 => "\x1bOP", 2 => "\x1bOQ", 3 => "\x1bOR", 4 => "\x1bOS",
                5 => "\x1b[15~", 6 => "\x1b[17~", 7 => "\x1b[18~", 8 => "\x1b[19~",
                9 => "\x1b[20~", 10 => "\x1b[21~", 11 => "\x1b[23~", 12 => "\x1b[24~",
                _ => return None,
            };
            Some(seq.as_bytes().to_vec())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_single_pane() {
        let layout = LayoutNode::Leaf { pane_id: 1 };
        assert_eq!(layout.pane_count(), 1);
        assert_eq!(layout.collect_pane_ids(), vec![1]);
    }

    #[test]
    fn test_layout_split_vertical() {
        let mut layout = LayoutNode::Leaf { pane_id: 1 };
        layout.split_pane(1, 2, SplitDirection::Vertical);
        assert_eq!(layout.pane_count(), 2);
        assert_eq!(layout.collect_pane_ids(), vec![1, 2]);
    }

    #[test]
    fn test_layout_split_horizontal() {
        let mut layout = LayoutNode::Leaf { pane_id: 1 };
        layout.split_pane(1, 2, SplitDirection::Horizontal);
        assert_eq!(layout.pane_count(), 2);
    }

    #[test]
    fn test_layout_nested_split() {
        let mut layout = LayoutNode::Leaf { pane_id: 1 };
        layout.split_pane(1, 2, SplitDirection::Vertical);
        layout.split_pane(1, 3, SplitDirection::Horizontal);
        assert_eq!(layout.pane_count(), 3);
        assert_eq!(layout.collect_pane_ids(), vec![1, 3, 2]);
    }

    #[test]
    fn test_layout_remove_pane() {
        let mut layout = LayoutNode::Leaf { pane_id: 1 };
        layout.split_pane(1, 2, SplitDirection::Vertical);
        layout.remove_pane(2);
        assert_eq!(layout.pane_count(), 1);
        assert_eq!(layout.collect_pane_ids(), vec![1]);
    }

    #[test]
    fn test_layout_remove_first_pane() {
        let mut layout = LayoutNode::Leaf { pane_id: 1 };
        layout.split_pane(1, 2, SplitDirection::Vertical);
        layout.remove_pane(1);
        assert_eq!(layout.collect_pane_ids(), vec![2]);
    }

    #[test]
    fn test_calculate_rects_vertical() {
        let layout = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf { pane_id: 1 }),
            second: Box::new(LayoutNode::Leaf { pane_id: 2 }),
        };
        let rects = layout.calculate_rects(Rect::new(0, 0, 100, 50));
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0], (1, Rect::new(0, 0, 50, 50)));
        assert_eq!(rects[1], (2, Rect::new(50, 0, 50, 50)));
    }

    #[test]
    fn test_calculate_rects_horizontal() {
        let layout = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf { pane_id: 1 }),
            second: Box::new(LayoutNode::Leaf { pane_id: 2 }),
        };
        let rects = layout.calculate_rects(Rect::new(0, 0, 100, 50));
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0], (1, Rect::new(0, 0, 100, 25)));
        assert_eq!(rects[1], (2, Rect::new(0, 25, 100, 25)));
    }

    #[test]
    fn test_focus_cycling() {
        let ids = vec![1, 2, 3];
        assert_eq!((0 + 1) % ids.len(), 1);
        assert_eq!((2 + 1) % ids.len(), 0);
    }
}
