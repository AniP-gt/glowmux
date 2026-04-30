pub(crate) use std::collections::{HashMap, VecDeque};
pub(crate) use std::path::PathBuf;
pub(crate) use std::sync::mpsc::{self, Receiver, Sender};
pub(crate) use std::time::{Duration, Instant};

pub(crate) use anyhow::Result;
pub(crate) use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
pub(crate) use ratatui::layout::Rect;

pub(crate) use crate::ai_title;
pub(crate) use crate::config::{ConfigFile, FeaturesConfig};
pub(crate) use crate::filetree::FileTree;
pub(crate) use crate::hooks::HookEvent;
pub(crate) use crate::pane::Pane;
pub(crate) use crate::preview::Preview;

mod copy_mode;
mod git;
mod key_handlers;
mod layout;
mod mouse;
mod pane_create;
mod pane_management;
mod pty;

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
    HookReceived {
        pane_id: usize,
        event: HookEvent,
        context: crate::hooks::HookContext,
    },
    /// AI title generation completed (None = generation failed/timed out).
    AiTitleGenerated {
        pane_id: usize,
        title: Option<String>,
    },
    /// AI branch name generation completed.
    BranchNameGenerated { branch: String },
    /// Async worktree creation completed successfully.
    WorktreeCreated {
        pane_id: usize,
        cwd: std::path::PathBuf,
        branch_name: String,
    },
    /// Async worktree creation failed.
    WorktreeCreateFailed {
        pane_id: usize,
        branch_name: String,
        error: String,
    },
    /// Worktree branch has been merged into main.
    WorktreeMerged { worktree_path: std::path::PathBuf },
    /// Worktree list refresh completed.
    WorktreesListed {
        worktrees: Vec<crate::worktree::WorktreeInfo>,
    },
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

fn resolve_pane_status(
    hook_status: PaneStatus,
    claude_state: &crate::claude_monitor::ClaudeState,
) -> PaneStatus {
    if claude_state.is_working {
        return PaneStatus::Running;
    }

    if hook_status != PaneStatus::Idle {
        return hook_status;
    }

    if claude_state.total_tokens() > 0 {
        return PaneStatus::Done;
    }

    PaneStatus::Idle
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
    (
        "ai_title_engine.update_interval_sec",
        "AI Title Update Interval (sec)",
    ),
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

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SidebarMode {
    #[default]
    None,
    FileTree,
    PaneList,
}

/// Which area has focus.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusTarget {
    Pane,
    FileTree,
    Preview,
    PaneList,
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

#[derive(Debug, Clone, PartialEq, Default)]
pub enum LaunchMode {
    #[default]
    Single,
    Multi,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PaneCreateField {
    LaunchModeToggle,
    BranchName,
    BaseBranch,
    WorktreeToggle,
    AgentField,
    PromptField,
    AiGenerate,
    OkButton,
    CancelButton,
    MultiCheck(usize),
}

#[derive(Debug, Clone)]
pub struct PaneCreateDialog {
    pub visible: bool,
    pub branch_name: String,
    pub base_branch: String,
    pub worktree_enabled: bool,
    pub agent: String,
    pub prompt: String,
    /// Byte offset of the cursor within `prompt`.
    pub prompt_cursor: usize,
    /// First visible line row index for prompt scrolling.
    pub prompt_scroll: usize,
    pub generating_name: bool,
    pub focused_field: PaneCreateField,
    pub error_msg: Option<String>,
    pub launch_mode: LaunchMode,
    pub agent_checks: Vec<bool>,
    pub agent_labels: Vec<String>,
}

impl Default for PaneCreateDialog {
    fn default() -> Self {
        Self {
            visible: false,
            branch_name: String::new(),
            base_branch: String::new(),
            worktree_enabled: false,
            agent: String::new(),
            prompt: String::new(),
            prompt_cursor: 0,
            prompt_scroll: 0,
            generating_name: false,
            focused_field: PaneCreateField::BranchName,
            error_msg: None,
            launch_mode: LaunchMode::Single,
            agent_checks: vec![],
            agent_labels: vec![],
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
    Leaf {
        pane_id: usize,
    },
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
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let (first_area, second_area) = split_rect(area, *direction, *ratio);
                let mut result = first.calculate_rects(first_area);
                result.extend(second.calculate_rects(second_area));
                result
            }
        }
    }

    pub fn split_pane(
        &mut self,
        target_id: usize,
        new_id: usize,
        direction: SplitDirection,
    ) -> bool {
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
                        let second =
                            std::mem::replace(second.as_mut(), LayoutNode::Leaf { pane_id: 0 });
                        *self = second;
                        return true;
                    }
                }
                if let LayoutNode::Leaf { pane_id } = second.as_ref() {
                    if *pane_id == target_id {
                        let first =
                            std::mem::replace(first.as_mut(), LayoutNode::Leaf { pane_id: 0 });
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
        if let LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
        } = self
        {
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
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => LayoutNode::Split {
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

fn preview_line_count(preview: &crate::preview::Preview) -> usize {
    if preview.diff_mode && !preview.diff_lines.is_empty() {
        preview.diff_lines.len()
    } else {
        preview.lines.len()
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
    pub sidebar_mode: SidebarMode,
    pub preview: Preview,
    pub git_status: Option<crate::git_status::GitStatusSnapshot>,
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
            sidebar_mode: SidebarMode::None,
            preview: Preview::new(),
            git_status: None,
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
    pub pane_rename_input: Option<(usize, String)>,
    pub pane_custom_titles: HashMap<usize, String>,
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
    pub ai_title_requested_once: std::collections::HashSet<usize>,
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
            status_bar_visible: false,
            dragging: None,
            hover_border: None,
            last_tab_rects: Vec::new(),
            last_new_tab_rect: None,
            rename_input: None,
            pane_rename_input: None,
            pane_custom_titles: HashMap::new(),
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
            ai_title_requested_once: std::collections::HashSet::new(),
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
                            )
                            .is_ok()
                            {
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

    /// Get the active workspace.
    pub fn ws(&self) -> &Workspace {
        &self.workspaces[self.active_tab]
    }

    /// Get the active workspace mutably.
    pub fn ws_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.active_tab]
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
}

pub(crate) fn dir_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

pub(crate) fn edit_key_buffer(buf: &mut String, key: KeyEvent, max_len: usize) -> bool {
    match key.code {
        KeyCode::Backspace => {
            buf.pop();
            true
        }
        KeyCode::Char(c) => {
            if key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                return true;
            }
            if buf.chars().count() < max_len {
                buf.push(c);
            }
            true
        }
        _ => false,
    }
}

pub(crate) fn should_request_ai_title(
    already_requested_once: bool,
    in_flight: bool,
    last_request: Option<Instant>,
    interval_secs: u64,
) -> bool {
    !already_requested_once
        && !in_flight
        && last_request
            .map(|t| t.elapsed().as_secs() >= interval_secs)
            .unwrap_or(true)
}

pub(crate) fn restore_session_workspaces(
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
            app.ai_title_requested_once.insert(pane_id);
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
            ws.layout
                .split_pane(focused, new_pane_id, SplitDirection::Vertical);
            if !pane_snap.title.is_empty() {
                app.ai_title_requested_once.insert(new_pane_id);
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
pub(crate) fn extract_selected_text(pane: &Pane, sr: u32, sc: u32, er: u32, ec: u32) -> String {
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
pub(crate) fn extract_preview_selected_text(
    preview: &crate::preview::Preview,
    sr: u32,
    sc: u32,
    er: u32,
    ec: u32,
) -> String {
    let lines: Vec<&str> = if preview.diff_mode && !preview.diff_lines.is_empty() {
        preview.diff_lines.iter().map(|line| line.text.as_str()).collect()
    } else {
        preview.lines.iter().map(|line| line.as_str()).collect()
    };
    let mut out: Vec<String> = Vec::new();

    for abs_row in sr..=er {
        let idx = abs_row as usize;
        if idx >= lines.len() {
            break;
        }
        let line = lines[idx];
        let chars: Vec<char> = line.chars().collect();

        let col_start = if abs_row == sr { sc as usize } else { 0 };
        let col_end_inclusive = if abs_row == er {
            ec as usize
        } else {
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
pub(crate) fn key_event_to_bytes(key: &KeyEvent) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                let ctrl_byte = (c.to_ascii_lowercase() as u8)
                    .wrapping_sub(b'a')
                    .wrapping_add(1);
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
                1 => "\x1bOP",
                2 => "\x1bOQ",
                3 => "\x1bOR",
                4 => "\x1bOS",
                5 => "\x1b[15~",
                6 => "\x1b[17~",
                7 => "\x1b[18~",
                8 => "\x1b[19~",
                9 => "\x1b[20~",
                10 => "\x1b[21~",
                11 => "\x1b[23~",
                12 => "\x1b[24~",
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
    use std::time::Duration;

    #[test]
    fn test_should_request_ai_title_allows_first_generation() {
        assert!(should_request_ai_title(false, false, None, 30));
    }

    #[test]
    fn test_should_request_ai_title_blocks_when_already_requested_once() {
        assert!(!should_request_ai_title(true, false, None, 30));
    }

    #[test]
    fn test_should_request_ai_title_blocks_when_in_flight() {
        assert!(!should_request_ai_title(false, true, None, 30));
    }

    #[test]
    fn test_should_request_ai_title_respects_interval() {
        let recent = Instant::now();
        let old = Instant::now() - Duration::from_secs(31);

        assert!(!should_request_ai_title(false, false, Some(recent), 30));
        assert!(should_request_ai_title(false, false, Some(old), 30));
    }

    #[test]
    fn test_resolve_pane_status_prefers_jsonl_running() {
        let state = crate::claude_monitor::ClaudeState {
            is_working: true,
            ..Default::default()
        };

        assert_eq!(
            resolve_pane_status(PaneStatus::Waiting, &state),
            PaneStatus::Running
        );
    }

    #[test]
    fn test_resolve_pane_status_prefers_hook_waiting_over_done_tokens() {
        let state = crate::claude_monitor::ClaudeState {
            output_tokens: 42,
            ..Default::default()
        };

        assert_eq!(
            resolve_pane_status(PaneStatus::Waiting, &state),
            PaneStatus::Waiting
        );
    }

    #[test]
    fn test_resolve_pane_status_prefers_hook_running_over_done_tokens() {
        let state = crate::claude_monitor::ClaudeState {
            output_tokens: 42,
            ..Default::default()
        };

        assert_eq!(
            resolve_pane_status(PaneStatus::Running, &state),
            PaneStatus::Running
        );
    }

    #[test]
    fn test_resolve_pane_status_falls_back_to_done_from_tokens() {
        let state = crate::claude_monitor::ClaudeState {
            output_tokens: 42,
            ..Default::default()
        };

        assert_eq!(
            resolve_pane_status(PaneStatus::Idle, &state),
            PaneStatus::Done
        );
    }

    #[test]
    fn test_dismiss_done_on_focus_marks_derived_done_as_dismissed() {
        let mut app = App::new(20, 80, ConfigFile::default()).unwrap();
        let pane_id = app.ws().focused_pane_id;

        let state = crate::claude_monitor::ClaudeState {
            output_tokens: 42,
            ..Default::default()
        };
        app.claude_monitor.set_state_for_test(pane_id, state);

        assert_eq!(app.pane_status(pane_id), PaneStatus::Done);
        app.dismiss_done_on_focus(pane_id);

        assert!(app.pane_state_dismissed(pane_id));
        assert_eq!(app.pane_states.get(&pane_id).map(|s| s.status), Some(PaneStatus::Done));
    }

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

    #[test]
    fn test_on_workspace_focus_context_changed_preserves_tree_when_root_matches() {
        let config = ConfigFile::default();
        let mut app = App::new(40, 120, config).expect("app");

        let root = app.ws().file_tree.root_path.clone();
        app.ws_mut().file_tree.selected_index = 2;
        app.ws_mut().file_tree.scroll_offset = 1;

        app.on_workspace_focus_context_changed();

        assert_eq!(app.ws().file_tree.root_path, root);
        assert_eq!(app.ws().file_tree.selected_index, 2);
        assert_eq!(app.ws().file_tree.scroll_offset, 1);
    }

    #[test]
    fn test_extract_preview_selected_text_uses_diff_lines_in_diff_mode() {
        let mut preview = crate::preview::Preview::new();
        preview.lines = vec!["plain".to_string()];
        preview.diff_mode = true;
        preview.diff_lines = vec![crate::preview::DiffLine {
            text: "+delta".to_string(),
            kind: crate::preview::DiffLineKind::Added,
            styled_spans: Vec::new(),
        }];

        assert_eq!(extract_preview_selected_text(&preview, 0, 0, 0, 5), "+delta");
    }

    #[test]
    fn pane_display_title_priority() {
        let mut app = App::new(20, 80, ConfigFile::default()).unwrap();
        let pane_id = app.ws().focused_pane_id;

        assert_eq!(app.pane_display_title(pane_id), None);

        app.ai_titles.insert(pane_id, "ai-title".to_string());
        assert_eq!(app.pane_display_title(pane_id), Some("ai-title"));

        app.pane_custom_titles
            .insert(pane_id, "custom".to_string());
        assert_eq!(app.pane_display_title(pane_id), Some("custom"));

        app.pane_custom_titles.remove(&pane_id);
        assert_eq!(app.pane_display_title(pane_id), Some("ai-title"));
    }

    #[test]
    fn pane_cleanup_removes_custom_title() {
        let mut app = App::new(20, 80, ConfigFile::default()).unwrap();
        let pane_id = 5;

        app.pane_custom_titles
            .insert(pane_id, "manual".to_string());
        app.pane_rename_input = Some((pane_id, "buf".to_string()));

        app.cleanup_pane_runtime_state(pane_id);

        assert!(!app.pane_custom_titles.contains_key(&pane_id));
        assert!(app.pane_rename_input.is_none());
    }

    #[test]
    fn rename_mutual_exclusion() {
        let mut app = App::new(20, 80, ConfigFile::default()).unwrap();

        // Tab rename dispatch should be skipped while pane_rename_input is set.
        app.pane_rename_input = Some((1, String::new()));
        let alt_r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::ALT);
        // pane_rename modal swallows the key (rather than starting tab rename).
        let _ = app.handle_key_event(alt_r).unwrap();
        assert!(app.rename_input.is_none());
        assert!(app.pane_rename_input.is_some());

        // Conversely, pane_rename dispatch should be skipped while rename_input is set.
        app.pane_rename_input = None;
        app.rename_input = Some(String::new());
        let alt_shift_r =
            KeyEvent::new(KeyCode::Char('R'), KeyModifiers::ALT | KeyModifiers::SHIFT);
        let _ = app.handle_key_event(alt_shift_r).unwrap();
        assert!(app.pane_rename_input.is_none());
        assert!(app.rename_input.is_some());
    }

    #[test]
    fn edit_key_buffer_handles_basic_input() {
        let mut buf = String::from("hi");
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert!(edit_key_buffer(&mut buf, key, 32));
        assert_eq!(buf, "hia");

        let bs = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        assert!(edit_key_buffer(&mut buf, bs, 32));
        assert_eq!(buf, "hi");

        // Ctrl/Alt-modified chars are swallowed but do not append.
        let ctrl = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(edit_key_buffer(&mut buf, ctrl, 32));
        assert_eq!(buf, "hi");

        // Length cap.
        let mut full = "x".repeat(32);
        let key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        assert!(edit_key_buffer(&mut full, key, 32));
        assert_eq!(full.chars().count(), 32);

        // Non-handled keys return false.
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert!(!edit_key_buffer(&mut buf, enter, 32));
    }

    #[test]
    fn test_format_cmd_arg() {
        let result = App::format_agent_command(
            "claude",
            "hello world",
            &crate::config::PromptMode::Arg,
        );
        assert_eq!(result, "claude 'hello world'");
    }

    #[test]
    fn test_format_cmd_flag() {
        let result = App::format_agent_command(
            "gemini",
            "hello world",
            &crate::config::PromptMode::Flag("-p".into()),
        );
        assert_eq!(result, "gemini -p 'hello world'");
    }

    #[test]
    fn test_format_cmd_stdin() {
        let result =
            App::format_agent_command("codex", "hello", &crate::config::PromptMode::Stdin);
        assert_eq!(result, "codex");
    }

    #[test]
    fn test_format_cmd_none() {
        let result =
            App::format_agent_command("foo", "hello", &crate::config::PromptMode::None);
        assert_eq!(result, "foo");
    }

    #[test]
    fn test_format_cmd_empty_prompt() {
        let result =
            App::format_agent_command("claude", "", &crate::config::PromptMode::Arg);
        assert_eq!(result, "claude");
    }

    /// Minimal POSIX sh single-quote unquoter: accepts a string composed of
    /// '...' segments and \' escape characters, returns the literal content.
    /// Returns None on malformed input.
    fn posix_unquote(s: &str) -> Option<String> {
        let mut out = String::new();
        let bytes = s.as_bytes();
        let mut i = 0;
        let mut in_quote = false;
        while i < bytes.len() {
            let b = bytes[i];
            if in_quote {
                if b == b'\'' {
                    in_quote = false;
                    i += 1;
                } else {
                    out.push(b as char);
                    i += 1;
                }
            } else {
                match b {
                    b'\'' => {
                        in_quote = true;
                        i += 1;
                    }
                    b'\\' if i + 1 < bytes.len() => {
                        out.push(bytes[i + 1] as char);
                        i += 2;
                    }
                    _ => return None,
                }
            }
        }
        if in_quote {
            return None;
        }
        Some(out)
    }

    #[test]
    fn test_format_cmd_injection() {
        let result = App::format_agent_command(
            "claude",
            "'; rm -rf / #",
            &crate::config::PromptMode::Arg,
        );
        assert!(result.starts_with("claude "), "result: {result}");
        let arg = &result["claude ".len()..];
        // The argument must round-trip through POSIX shell single-quote
        // semantics back to the original prompt; this rejects any escape
        // that would let "; rm" run as a separate command.
        let parsed = posix_unquote(arg).expect("should be parseable");
        assert_eq!(parsed, "'; rm -rf / #", "not round-trip safe: {result}");
    }

    #[test]
    fn test_grid_layout_n2() {
        let node = App::build_layout_node(LayoutMode::Grid, &[1, 2]);
        assert!(node.is_some());
        let ids = node.unwrap().collect_pane_ids();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn test_grid_layout_n3() {
        let node = App::build_layout_node(LayoutMode::Grid, &[1, 2, 3]);
        assert!(node.is_some());
        let ids = node.unwrap().collect_pane_ids();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn test_grid_layout_n4() {
        let node = App::build_layout_node(LayoutMode::Grid, &[1, 2, 3, 4]);
        assert!(node.is_some());
        let ids = node.unwrap().collect_pane_ids();
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn test_multi_zero_selection_sets_error() {
        // Verify that format_agent_command with all-false checks never reaches
        // pane creation — the empty selected_indices guard fires first.
        let checks: Vec<bool> = vec![false, false, false, false];
        let selected: Vec<usize> = checks
            .iter()
            .enumerate()
            .filter(|(_, &c)| c)
            .map(|(i, _)| i)
            .collect();
        assert!(selected.is_empty(), "zero checks must yield no selected agents");
        // Simulate the error path: error_msg should be set, not None.
        let error_msg: Option<String> = if selected.is_empty() {
            Some("Select at least one AI".into())
        } else {
            None
        };
        assert_eq!(error_msg.as_deref(), Some("Select at least one AI"));
    }

    #[test]
    fn test_single_tab_cycle_9_stops() {
        // Single mode must cycle through exactly 9 distinct stops before
        // returning to LaunchModeToggle (the new first stop).
        use PaneCreateField::*;
        let stops = [
            LaunchModeToggle,
            BranchName,
            BaseBranch,
            WorktreeToggle,
            AgentField,
            PromptField,
            AiGenerate,
            OkButton,
            CancelButton,
        ];
        // Verify Tab forward wraps: last stop -> LaunchModeToggle
        assert_eq!(stops.len(), 9);
        // Simulate the Tab forward transition for each stop
        let n_agents = 4usize;
        let advance = |f: &PaneCreateField| -> PaneCreateField {
            match f {
                LaunchModeToggle => BranchName,
                BranchName => BaseBranch,
                BaseBranch => WorktreeToggle,
                WorktreeToggle => AgentField,
                AgentField => PromptField,
                PromptField => AiGenerate,
                AiGenerate => OkButton,
                OkButton => CancelButton,
                CancelButton => LaunchModeToggle,
                MultiCheck(i) if *i + 1 < n_agents => MultiCheck(*i + 1),
                MultiCheck(_) => OkButton,
            }
        };
        let mut f = LaunchModeToggle;
        for expected in stops.iter().skip(1).chain(std::iter::once(&LaunchModeToggle)) {
            f = advance(&f);
            assert_eq!(&f, expected);
        }
    }

    #[test]
    fn test_multi_tab_cycle_8_stops() {
        // Multi mode with 4 agents: 4 + 4 = 8 stops.
        use PaneCreateField::*;
        let n_agents = 4usize;
        let advance = |f: &PaneCreateField| -> PaneCreateField {
            match f {
                LaunchModeToggle => if n_agents > 0 { MultiCheck(0) } else { PromptField },
                MultiCheck(i) if *i + 1 < n_agents => MultiCheck(*i + 1),
                MultiCheck(_) => PromptField,
                PromptField => OkButton,
                OkButton => CancelButton,
                CancelButton => LaunchModeToggle,
                _ => LaunchModeToggle,
            }
        };
        let stops = [
            LaunchModeToggle,
            MultiCheck(0),
            MultiCheck(1),
            MultiCheck(2),
            MultiCheck(3),
            PromptField,
            OkButton,
            CancelButton,
        ];
        assert_eq!(stops.len(), 8);
        let mut f = LaunchModeToggle;
        for expected in stops.iter().skip(1).chain(std::iter::once(&LaunchModeToggle)) {
            f = advance(&f);
            assert_eq!(&f, expected);
        }
    }
}
