//! Params and results of `pane.*`.

use crate::{
    api::{
        common::{Bytes, LineRange, ProcessSpec, Rect, SplitDirection, wire},
        ids::{PaneId, Ref, TabId, WindowId, WorkspaceId},
        model::PaneValue,
    },
    input::LogicalInput,
};

wire! {
    pub struct PaneSplit {
        /// The pane to split.
        pub pane: Ref<PaneId>,
        /// Which side of `pane` the new pane goes on.
        pub direction: SplitDirection,
        /// The share of the new pane, from 0 to 1. Without it the two panes share the room.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub ratio: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub command: Option<ProcessSpec>,
    }

    pub struct PaneCreated {
        pub pane: PaneId,
        pub handle: String,
    }

    /// `confirm_required` when the child is alive and `force` is false. `details` has
    /// `{process, is_agent}`.
    pub struct PaneClose {
        pub pane: Ref<PaneId>,
        #[serde(default)]
        pub force: bool,
    }

    /// Keyboard focus moved to a pane in a window.
    pub struct PaneFocus {
        pub window: WindowId,
        pub pane: Ref<PaneId>,
    }

    /// The window takes the geometry lease of the pane as an input does, with the same debounce,
    /// and sends no byte. A phone calls it when its keyboard opens.
    pub struct PaneClaim {
        pub pane: Ref<PaneId>,
        pub window: WindowId,
    }

    /// Moves a pane to another tab. Without `beside` the pane becomes a split of the root.
    pub struct PaneMove {
        pub pane: Ref<PaneId>,
        pub tab: Ref<TabId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub beside: Option<Ref<PaneId>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub direction: Option<SplitDirection>,
    }

    pub struct PaneFloat {
        pub pane: Ref<PaneId>,
        /// Where to put it. Without it the core picks a rectangle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub rect: Option<Rect>,
    }

    pub struct PaneUnfloat {
        pub pane: Ref<PaneId>,
    }

    /// At most once for each animation frame while the user drags.
    pub struct PaneSetFloatRect {
        pub pane: Ref<PaneId>,
        pub rect: Rect,
    }

    /// Moves a pane into a window of its own.
    pub struct PanePopout {
        pub pane: Ref<PaneId>,
    }

    /// Starts the pane again after its child exited.
    pub struct PaneRestart {
        pub pane: Ref<PaneId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub command: Option<ProcessSpec>,
    }

    /// From the `ResizeObserver`. The geometry lease decides whether it resizes the PTY.
    pub struct PaneResize {
        pub pane: Ref<PaneId>,
        pub cols: u16,
        pub rows: u16,
        pub window: WindowId,
    }

    /// One logical event, at once. `input_id` is unique for the core epoch. A holder
    /// drops a repeated id.
    pub struct PaneInput {
        pub pane: Ref<PaneId>,
        pub input_id: u64,
        pub event: LogicalInput,
    }

    pub struct PanePaste {
        pub pane: Ref<PaneId>,
        pub input_id: u64,
        pub text: String,
    }

    /// Raw bytes, as the explicit exception to logical input.
    pub struct PaneSendBytes {
        pub pane: Ref<PaneId>,
        pub input_id: u64,
        pub bytes: Bytes,
    }

    /// A file that a client put in the blob store (`BlobPut`), with the name it had.
    pub struct DroppedBlob {
        /// The BLAKE3 digest in hex that the upload answered.
        pub hash: String,
        pub name: String,
    }

    /// Files dropped on a pane of a remote window. The client has put each file in the
    /// blob store over its link. The core writes each blob to a file of the machine of the pane,
    /// then quotes the paths and pastes them as `pane.drop_files` does.
    pub struct PaneDropBlobs {
        pub pane: Ref<PaneId>,
        pub input_id: u64,
        pub blobs: Vec<DroppedBlob>,
    }

    /// The core quotes the paths and pastes them.
    pub struct PaneDropFiles {
        pub pane: Ref<PaneId>,
        pub input_id: u64,
        pub paths: Vec<String>,
    }

    /// Paths of the machine of the core, from the Files tab. The core quotes and pastes
    /// them as `pane.drop_files` does, and the host never uploads them.
    pub struct PaneInsertPaths {
        pub pane: Ref<PaneId>,
        pub input_id: u64,
        /// 1 to 20 absolute paths.
        pub paths: Vec<String>,
    }

    /// A viewport change. Frames follow.
    #[serde(untagged)]
    pub enum ScrollTo {
        /// The absolute line at the top of the viewport.
        TopLine { top_line: u64 },
        /// Lines up (negative) or down (positive).
        Delta { delta: i64 },
    }

    pub struct PaneScroll {
        pub pane: Ref<PaneId>,
        #[serde(flatten)]
        pub to: ScrollTo,
    }

    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum SelectMode {
        Simple,
        Word,
        Line,
        Block,
    }

    /// A cell in the scrollback: the column, and the absolute line.
    #[derive(Copy, Eq, Hash)]
    pub struct SelectPoint {
        pub col: u16,
        pub line: u64,
    }

    /// The selection lives in the core.
    pub struct PaneSelect {
        pub pane: Ref<PaneId>,
        pub anchor: SelectPoint,
        pub head: SelectPoint,
        pub mode: SelectMode,
    }

    pub struct PaneSelectClear {
        pub pane: Ref<PaneId>,
    }

    /// Writes the selection to the clipboard through the host.
    pub struct PaneCopy {
        pub pane: Ref<PaneId>,
        /// Also return the text, for the CLI and for a phone.
        ///
        /// A phone writes its own clipboard in the tap, so the core writes none for it.
        #[serde(default)]
        pub return_text: bool,
    }

    pub struct CopyResult {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub text: Option<String>,
    }

    /// Reads text from a pane. Without `lines` it reads the visible screen.
    pub struct PaneRead {
        pub pane: Ref<PaneId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub lines: Option<LineRange>,
        /// Read the visible screen. It is the default when `lines` is absent.
        #[serde(default)]
        pub visible: bool,
    }

    pub struct PaneText {
        pub text: String,
        /// The absolute line of the first line of `text`.
        pub from_line: u64,
    }

    /// Starts low-rate frames for a pane that is not in the grid.
    pub struct PanePeekOpen {
        pub pane: Ref<PaneId>,
    }

    pub struct PanePeekClose {
        pub pane: Ref<PaneId>,
    }

    /// The answer to an OSC 52 clipboard write.
    pub struct PaneClipboardAllow {
        pub pane: Ref<PaneId>,
        pub allow: bool,
        /// Later writes of this pane follow the answer without asking.
        #[serde(default)]
        pub remember: bool,
        /// Return the text, and write no clipboard of a host: a phone writes it in the tap.
        #[serde(default)]
        pub return_text: bool,
    }

    pub struct PaneList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub workspace: Option<Ref<WorkspaceId>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub tab: Option<Ref<TabId>>,
    }

    pub struct PaneListResult {
        pub panes: Vec<PaneValue>,
    }

    pub struct PaneShow {
        pub pane: Ref<PaneId>,
    }

    pub struct PaneShown {
        pub pane: PaneValue,
    }
}
